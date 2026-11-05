#!/usr/bin/env python3
"""Compare fish's live input highlighting with eza in an isolated PTY.
Requires pyte and pexpect; pass --fish and --eza explicitly.
"""
import argparse
import os
import re
from pathlib import Path
import shlex
import socket
import statistics
import subprocess
import tempfile
import time

import pexpect
import pyte


def appearance(cell):
    return cell.fg, cell.bg, cell.bold, cell.italics, cell.reverse, cell.strikethrough


class Shell:
    def __init__(self, binary, directory, environment, plugin=None):
        init = ("function fish_prompt; printf 'P> '; end; "
                "set -g fish_greeting; set -g fish_color_path_by_type 1; "
                "set -g fish_color_command green; set -g fish_color_param white; "
                "set -g fish_color_valid_path --underline; set -g fish_autosuggestion_enabled 0")
        if plugin is not None:
            init += "; source " + shlex.quote(str(plugin))
        self.process = pexpect.spawn(binary, ['--no-config', '--interactive', '-C', init],
                                    cwd=directory, env=environment, encoding='utf-8',
                                    dimensions=(24, 160), echo=False)
        self.process.delaybeforesend = 0
        self.screen = pyte.Screen(160, 24)
        self.stream = pyte.Stream(self.screen)
        self.pending = ""
        self.pump(0.5)

    def pump(self, duration):
        end = time.monotonic() + duration
        while time.monotonic() < end:
            try:
                data = self.process.read_nonblocking(32768, timeout=0.01)
            except pexpect.TIMEOUT:
                continue
            # pyte mistakes xterm's private modifyOtherKeys mode for SGR bold.
            # Retain an incomplete CSI across reads before filtering it out.
            chunk = self.pending + data
            self.pending = ""
            partial = re.search(r"\x1b(?:\[[0-9;>?]*)?$", chunk)
            if partial:
                self.pending = chunk[partial.start():]
                chunk = chunk[:partial.start()]
            self.stream.feed(re.sub(r"\x1b\[>[0-9;]*m", "", chunk))
            if '\x1b[6n' in data:
                self.process.send(f'\x1b[{self.screen.cursor.y + 1};{self.screen.cursor.x + 1}R')
            if '\x1b[c' in data or '\x1b[0c' in data:
                self.process.send('\x1b[?1;2c')
            if '\x1b[?2004$p' in data:
                self.process.send('\x1b[?2004;1$y')

    def cell_for(self, token):
        needle = 'cat ' + token
        for row in range(self.screen.lines - 1, -1, -1):
            line = self.screen.display[row]
            start = line.find(needle)
            if start >= 0:
                offset = 1 if token.startswith(("'", '"')) else 0
                return self.screen.buffer[row][start + 4 + offset]
        return None

    def check(self, token, expected):
        self.process.sendcontrol('u')
        self.pump(0.03)
        start = time.monotonic()
        self.process.send('cat ' + token)
        for _ in range(60):
            self.pump(0.01)
            cell = self.cell_for(token)
            if cell and appearance(cell) == expected:
                return (time.monotonic() - start) * 1000
        raise AssertionError(f'{token}: expected {expected}, got {appearance(cell) if cell else None}\n'
                             + '\n'.join(self.screen.display))

    def execute(self, line):
        self.process.sendcontrol('u')
        self.process.sendline(line)
        self.pump(0.2)

    def close(self):
        self.process.close(force=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--fish', required=True)
    parser.add_argument('--eza', required=True)
    parser.add_argument('--plugin', required=True, help='Packaged plugin conf.d file')
    args = parser.parse_args()
    with tempfile.TemporaryDirectory(prefix='fish-eza-check-') as temporary:
        root = Path(temporary)
        home = root / 'home'
        home.mkdir()
        work = root / 'files'
        work.mkdir()
        (work / 'directory').mkdir()
        names = ['plain', 'source.rs', 'picture.png', 'video.mp4', 'music.mp3', 'lossless.flac',
                 'key.pem', 'document.pdf', 'archive.zip', 'scratch~', 'compiled.o',
                 'Cargo.toml', 'README.md', 'space name.png', '-option.png', 'run.sh']
        for name in names:
            (work / name).touch()
        (work / 'run.sh').chmod(0o755)
        (work / 'link').symlink_to('plain')
        (work / 'link-dir').symlink_to('directory')
        (work / 'broken').symlink_to('missing')
        os.mkfifo(work / 'pipe')
        sock = socket.socket(socket.AF_UNIX)
        sock.bind(str(work / 'socket'))
        env = dict(os.environ, HOME=str(home), XDG_CONFIG_HOME=str(home / '.config'),
                   TERM='xterm-256color', COLORTERM='truecolor', LC_ALL='C.UTF-8')
        for name in ['EZA_COLORS', 'LS_COLORS', 'EXA_COLORS', 'EZA_CONFIG_DIR', 'TUIOS_WINDOW_ID']:
            env.pop(name, None)

        def expected(name, environment=env):
            output = subprocess.check_output([args.eza, '--oneline', '--treat-dirs-as-files',
                                              '--icons=never', '--color=always', '--no-quotes',
                                              '--', name], cwd=work, env=environment).decode()
            screen = pyte.Screen(160, 24)
            pyte.Stream(screen).feed(output)
            # All test paths are simple names except /dev/null, whose prefix eza colors separately.
            column = screen.display[0].find(Path(name).name)
            return appearance(screen.buffer[0][max(column, 0)])

        shell = Shell(args.fish, work, env, args.plugin)
        try:
            timings = []
            for name in ['directory', *names, 'link', 'link-dir', 'broken', 'pipe', 'socket', '/dev/null']:
                token = shlex.quote(name)
                if name.startswith('-'):
                    token = './' + name
                if name == '/dev/null':
                    # fish intentionally colors a whole path by its final entry's type.
                    token = name
                timings.append(shell.check(token, expected(name)))
            # Same-path repaint uses the cached eza classification.
            cached = [shell.check('picture.png', expected('picture.png')) for _ in range(3)]
            shell.check('link-dir/', expected('link-dir'))
            custom = 'di=38;2;120;140;200:*.png=1;38;5;208'
            shell.execute('set -gx EZA_COLORS ' + shlex.quote(custom))
            custom_env = dict(env, EZA_COLORS=custom)
            shell.check('directory', expected('directory', custom_env))
            shell.check('picture.png', expected('picture.png', custom_env))
            # An explicit history-search color must win over the filename color.
            shell.execute('cat picture.png')
            shell.execute('set -g fish_color_search_match --background=magenta red')
            shell.process.send('cat pic')
            shell.pump(0.05)
            shell.process.send('\x1b[A')
            shell.pump(0.15)
            cell = shell.cell_for('picture.png')
            assert cell is not None and cell.fg == 'red' and cell.bg == 'magenta', 'history search lost its highlighting'
            # Merely typing a command substitution must never execute it.
            sentinel = root / 'must-not-exist'
            shell.process.sendcontrol('u')
            shell.process.send('cat $(touch ' + str(sentinel) + ')')
            shell.pump(0.2)
            assert not sentinel.exists(), 'highlighting executed a command substitution'
            shell.execute('set -g fish_color_path_by_type 0')
            shell.check('directory', ('white', 'default', False, False, False, False))
            print(f'PASS: 23 path cases, eza overrides, history search, safe substitutions, disable switch; '
                  f'cold median={statistics.median(timings):.0f}ms, '
                  f'cached median={statistics.median(cached):.0f}ms')
        finally:
            shell.close()
            sock.close()


if __name__ == '__main__':
    main()

#!/usr/bin/env python3
"""Check stock-fish fallback and the versioned provider boundary in an isolated PTY."""
import argparse
import importlib.util
import os
from pathlib import Path
import shlex
import subprocess
import sys
import tempfile
import time


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--fish', required=True)
    parser.add_argument('--stock-fish', required=True)
    parser.add_argument('--plugin', required=True)
    parser.add_argument('--provider', required=True)
    args = parser.parse_args()
    spec = importlib.util.spec_from_file_location('path_checks', Path(__file__).with_name('check-fish-path-colors.py'))
    checks = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(checks)
    assert subprocess.check_output([args.provider, '--protocol-version'], text=True).strip() == '1'
    assert subprocess.run([args.provider, '--fish-path-style-v2', '/'], capture_output=True).returncode != 0
    with tempfile.TemporaryDirectory(prefix='fish-provider-compat-') as temporary:
        root = Path(temporary)
        (root/'directory').mkdir()
        env = dict(os.environ, HOME=str(root), XDG_CONFIG_HOME=str(root/'.config'), TERM='xterm-256color')
        for name in ['fish_path_style_provider_protocol', 'fish_color_path_style_provider',
                     'fish_color_path_style_provider_enabled', 'fish_color_path_by_type', 'TUIOS_WINDOW_ID']:
            env.pop(name, None)
        fallback = ('source ' + shlex.quote(args.plugin) +
                    '; set -q fish_color_path_style_provider; and exit 1; printf STOCK_OK')
        result = subprocess.run([args.stock_fish, '--no-config', '-c', fallback], env=env, capture_output=True, text=True, check=True)
        assert result.stdout == 'STOCK_OK' and not result.stderr, result
        child_code = 'set -q fish_path_style_provider_protocol; and exit 1; printf CHILD_OK'
        code = ("test \"$fish_path_style_provider_protocol\" = 1; or exit 2; " +
                shlex.quote(args.stock_fish) + ' --no-config -c ' + shlex.quote(child_code))
        result = subprocess.run([args.fish, '--no-config', '-c', code], env=env, capture_output=True, text=True, check=True)
        assert result.stdout == 'CHILD_OK', result

        def fixture(name, body):
            path = root/name
            path.write_text('#!' + sys.executable + '\n' + body + '\n')
            path.chmod(0o755)
            return path

        invalid = [
            fixture('wrong-version', 'print("FISH_PATH_STYLE 2 5 0 1")'),
            fixture('terminal-controls', 'print("\\x1b[34mFISH_PATH_STYLE 1 5 0 1")'),
            fixture('invalid-color', 'print("FISH_PATH_STYLE 1 257 0 1")'),
            fixture('extra-fields', 'print("FISH_PATH_STYLE 1 5 0 1 ignored")'),
            fixture('failure', 'raise SystemExit(1)'),
            fixture('oversized', 'print("x" * 20000)'),
        ]
        valid = fixture('valid-v1', 'print("FISH_PATH_STYLE 1 5 0 1")')
        pidfile = root/'helper.pid'
        slow = fixture('slow', 'import subprocess,sys,time\n'
                       'child=subprocess.Popen([sys.executable,"-c","import time;time.sleep(20)"])\n'
                       f'open({str(pidfile)!r},"w").write(str(child.pid))\n'
                       'time.sleep(20)')
        shell = checks.Shell(args.fish, root, env, args.plugin)
        try:
            normal = ('white', 'default', False, False, False, False)
            blue = ('blue', 'default', True, False, False, False)
            def check_style(expected):
                shell.check('directory', expected)
                shell.pump(0.35)
                assert checks.appearance(shell.cell_for('directory')) == expected
            shell.execute('set -g fish_color_path_style_provider ' + shlex.quote(str(valid)))
            check_style(blue)
            for provider in [*invalid, slow, root/'missing']:
                shell.execute('set -g fish_color_path_style_provider ' + shlex.quote(str(provider)))
                check_style(normal)
            assert pidfile.exists(), 'slow fixture was not exercised'
            pid = int(pidfile.read_text())
            # A killed helper may briefly be a zombie while its adopter reaps it.
            stat = Path(f'/proc/{pid}/stat')
            for _ in range(20):
                if not stat.exists() or stat.read_text().split()[2] in ['Z', 'X']:
                    break
                time.sleep(0.05)
            else:
                raise AssertionError('provider helper survived its timeout')
            shell.execute('set -g fish_color_path_style_provider ' + shlex.quote(str(valid)))
            shell.execute('set -g fish_color_path_by_type 0')
            check_style(normal)
            shell.execute('set -g fish_color_path_by_type 1')
            check_style(blue)
            shell.execute('set -g fish_eza_path_colors_enabled 0')
            check_style(normal)
        finally:
            shell.close()
    print('PASS: stock fallback, non-inherited capability, v1 provider, rejected replies, timeout group, both switches')


if __name__ == '__main__':
    main()

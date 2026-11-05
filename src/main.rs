//! Independent eza provider for the fish path-style protocol, version 1.
use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

// 0 = terminal default, 1..256 = palette, 0x1000000 | RGB = truecolor.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
struct Style {
    fg: u32,
    bg: u32,
    flags: u8,
}

impl Style {
    fn sgr(&mut self, codes: &[u16]) -> Option<()> {
        let mut i = 0;
        while i < codes.len() {
            match codes[i] {
                0 => *self = Self::default(),
                1 => self.flags |= 1,
                2 => self.flags |= 2,
                3 => self.flags |= 4,
                4 => self.flags |= 8,
                7 => self.flags |= 16,
                9 => self.flags |= 32,
                22 => self.flags &= !3,
                23 => self.flags &= !4,
                24 => self.flags &= !8,
                27 => self.flags &= !16,
                29 => self.flags &= !32,
                n @ 30..=37 => self.fg = u32::from(n - 30 + 1),
                n @ 40..=47 => self.bg = u32::from(n - 40 + 1),
                n @ 90..=97 => self.fg = u32::from(n - 90 + 9),
                n @ 100..=107 => self.bg = u32::from(n - 100 + 9),
                39 => self.fg = 0,
                49 => self.bg = 0,
                n @ (38 | 48) => {
                    let c = match *codes.get(i + 1)? {
                        5 => {
                            i += 2;
                            u32::from(*codes.get(i)?.min(&255)) + 1
                        }
                        2 => {
                            let r = *codes.get(i + 2)?;
                            let g = *codes.get(i + 3)?;
                            let b = *codes.get(i + 4)?;
                            if r > 255 || g > 255 || b > 255 {
                                return None;
                            }
                            i += 4;
                            0x1000000 | u32::from(r) << 16 | u32::from(g) << 8 | u32::from(b)
                        }
                        _ => return None,
                    };
                    if n == 38 {
                        self.fg = c;
                    } else {
                        self.bg = c;
                    }
                }
                _ => (),
            }
            i += 1;
        }
        Some(())
    }
}

// Only decode leading SGR; never relay terminal controls or filename bytes.
fn parse_style(bytes: &[u8], name: &[u8]) -> Option<Style> {
    let mut style = Style::default();
    let mut i = 0;
    while bytes.get(i..i + 2) == Some(b"\x1b[") {
        let end = i + 2 + bytes.get(i + 2..)?.iter().position(|b| *b == b'm')?;
        let codes = std::str::from_utf8(bytes.get(i + 2..end)?).ok()?;
        let codes: Option<Vec<u16>> = codes
            .split(';')
            .map(|n| {
                if n.is_empty() {
                    Some(0)
                } else {
                    n.parse().ok()
                }
            })
            .collect();
        style.sgr(&codes?)?;
        i = end + 1;
    }
    bytes.get(i..)?.starts_with(name).then_some(style)
}

fn query(path: &Path) -> Option<Style> {
    use std::os::unix::ffi::OsStrExt;
    if !path.is_absolute() {
        return None;
    }
    let parent = path.parent().unwrap_or(Path::new("/"));
    let name = path.file_name().unwrap_or(std::ffi::OsStr::new("."));
    if name.as_bytes().iter().any(|b| *b < 32 || *b == 127) {
        return None;
    }
    let mut child = Command::new(option_env!("EZA_EXECUTABLE").unwrap_or("eza"))
        .current_dir(parent)
        .args([
            "--oneline",
            "--treat-dirs-as-files",
            "--color=always",
            "--icons=never",
            "--hyperlink=never",
            "--classify=never",
            "--no-quotes",
            "--",
        ])
        .arg(name)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                if !status.success() {
                    return None;
                }
                let mut output = Vec::new();
                child
                    .stdout
                    .take()?
                    .take(16384)
                    .read_to_end(&mut output)
                    .ok()?;
                return parse_style(&output, name.as_bytes());
            }
            Ok(None) if started.elapsed() < Duration::from_millis(60) => {
                std::thread::sleep(Duration::from_millis(1))
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    }
}

fn main() {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() == 1 && args[0] == "--protocol-version" {
        println!("1");
        return;
    }
    if args.len() != 2 || args[0] != "--fish-path-style-v1" {
        std::process::exit(2);
    }
    if let Some(style) = query(Path::new(&args[1])) {
        println!(
            "FISH_PATH_STYLE 1 {} {} {}",
            style.fg, style.bg, style.flags
        );
    } else {
        std::process::exit(1);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn eza_provider_sgr() {
        let style = parse_style(b"\x1b[1;34mGit\x1b[0m\n", b"Git").unwrap();
        assert_eq!(style.fg, 5);
        assert_eq!(style.flags, 1);
        let style = parse_style(
            b"\x1b[38;2;100;150;200;48;5;17mphoto.png\x1b[0m\n",
            b"photo.png",
        )
        .unwrap();
        assert_eq!(style.fg, 0x16496c8);
        assert_eq!(style.bg, 18);
        assert!(parse_style(b"\x1b]52;c;bad\x07file", b"file").is_none());
        assert!(parse_style(b"\x1b[38;2;1mfile", b"file").is_none());
        assert!(parse_style(b"wrong", b"file").is_none());
    }
}

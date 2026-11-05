//! Optional external path-style providers. Protocol v1 has no fish/Rust ABI dependency.
use crate::env::Environment;
use crate::text_face::{ResettableStyle, TextFace, UnderlineStyle};
use fish_color::Color;
use fish_widestring::{L, wcs2osstring, wstr};
use std::collections::HashMap;
use std::ffi::OsString;
use std::io::Read;
use std::os::unix::{fs::MetadataExt, process::CommandExt};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

// Keep HighlightSpec Copy/Eq/Hash for fish's existing render cache.
// 0 = terminal default, 1..256 = palette, 0x1000000 | RGB = truecolor.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct PathStyle {
    fg: u32,
    bg: u32,
    flags: u8,
}

impl PathStyle {
    pub(crate) fn apply(self, face: &mut TextFace) {
        let color = |n: u32| match n {
            0 => Color::Normal,
            1..=256 => Color::Named { idx: (n - 1) as u8 },
            _ => Color::from_rgb((n >> 16) as u8, (n >> 8) as u8, n as u8),
        };
        face.fg = color(self.fg);
        face.bg = color(self.bg);
        face.style.bold = self.flags & 1 != 0;
        face.style.dim = self.flags & 2 != 0;
        face.style.italics = if self.flags & 4 != 0 {
            ResettableStyle::On(())
        } else {
            ResettableStyle::Off
        };
        if self.flags & 8 != 0 {
            face.style
                .inject_underline(ResettableStyle::On(UnderlineStyle::Single));
        }
        face.style.reverse = if self.flags & 16 != 0 {
            ResettableStyle::On(())
        } else {
            ResettableStyle::Off
        };
        face.style.strikethrough = if self.flags & 32 != 0 {
            ResettableStyle::On(())
        } else {
            ResettableStyle::Off
        };
    }
}

fn parse_style(bytes: &[u8]) -> Option<PathStyle> {
    let words: Vec<_> = std::str::from_utf8(bytes)
        .ok()?
        .split_ascii_whitespace()
        .collect();
    if words.len() != 5 || words[0] != "FISH_PATH_STYLE" || words[1] != "1" {
        return None;
    }
    let fg: u32 = words[2].parse().ok()?;
    let bg: u32 = words[3].parse().ok()?;
    let flags: u8 = words[4].parse().ok()?;
    let valid_color = |c| c <= 256 || (0x1000000..=0x1ffffff).contains(&c);
    (valid_color(fg) && valid_color(bg) && flags <= 63).then_some(PathStyle { fg, bg, flags })
}

#[derive(Hash, Eq, PartialEq)]
struct CacheKey {
    path: PathBuf,
    provider: PathBuf,
    metadata: (u64, u32, i64, i64, i64, i64),
    environment: Vec<(OsString, Option<OsString>)>,
}
struct CacheEntry {
    at: Instant,
    style: Option<PathStyle>,
}
static CACHE: OnceLock<Mutex<HashMap<CacheKey, CacheEntry>>> = OnceLock::new();

pub(crate) fn style_for(path: &wstr, vars: &dyn Environment) -> Option<PathStyle> {
    if vars
        .get(L!("fish_color_path_style_provider_enabled"))?
        .as_string()
        != L!("1")
    {
        return None;
    }
    let provider = PathBuf::from(wcs2osstring(
        &vars
            .get_unless_empty(L!("fish_color_path_style_provider"))?
            .as_string(),
    ));
    if !provider.is_absolute() {
        return None;
    }
    let path = PathBuf::from(wcs2osstring(path));
    if !path.is_absolute() {
        return None;
    }
    let meta = std::fs::symlink_metadata(&path).ok()?;
    let parent_meta = std::fs::metadata(path.parent().unwrap_or(&path)).ok()?;
    let mut environment = Vec::new();
    if let Some(names) = vars.get(L!("fish_color_path_style_provider_environment")) {
        for name in names.as_list() {
            if !crate::common::valid_var_name(name) {
                return None;
            }
            environment.push((
                wcs2osstring(name),
                vars.get(name).map(|v| wcs2osstring(&v.as_string())),
            ));
        }
    }
    let key = CacheKey {
        path: path.clone(),
        provider: provider.clone(),
        environment,
        metadata: (
            meta.ino(),
            meta.mode(),
            meta.ctime(),
            meta.ctime_nsec(),
            parent_meta.mtime(),
            parent_meta.mtime_nsec(),
        ),
    };
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(entry) = cache.lock().ok()?.get(&key) {
        if entry.at.elapsed() < Duration::from_secs(2) {
            return entry.style;
        }
    }
    let mut command = Command::new(provider);
    command
        .args([
            std::ffi::OsStr::new("--fish-path-style-v1"),
            path.as_os_str(),
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .process_group(0);
    for (name, value) in &key.environment {
        if let Some(value) = value {
            command.env(name, value);
        } else {
            command.env_remove(name);
        }
    }
    let mut child = command.spawn().ok()?;
    let started = Instant::now();
    let style = loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let mut output = Vec::new();
                if status.success() {
                    if let Some(stdout) = child.stdout.take() {
                        let _ = stdout.take(129).read_to_end(&mut output);
                    }
                }
                break if output.len() <= 128 {
                    parse_style(&output)
                } else {
                    None
                };
            }
            Ok(None) if started.elapsed() < Duration::from_millis(80) => {
                std::thread::sleep(Duration::from_millis(1))
            }
            _ => {
                // Kill the provider's group, including any helper it started.
                unsafe {
                    libc::kill(-(child.id() as i32), libc::SIGKILL);
                }
                let _ = child.wait();
                break None;
            }
        }
    };
    let mut cache = cache.lock().ok()?;
    if cache.len() >= 256 {
        cache.clear();
    }
    cache.insert(
        key,
        CacheEntry {
            at: Instant::now(),
            style,
        },
    );
    style
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn path_style_protocol() {
        assert_eq!(parse_style(b"FISH_PATH_STYLE 1 5 0 1\n").unwrap().fg, 5);
        assert!(parse_style(b"FISH_PATH_STYLE 1 23369472 256 63").is_some());
        for invalid in [
            "FISH_PATH_STYLE 2 5 0 1",
            "FISH_PATH_STYLE 1 257 0 1",
            "FISH_PATH_STYLE 1 0 0 64",
            "FISH_PATH_STYLE 1 0 0 0 extra",
            "\x1b[34mFISH_PATH_STYLE 1 5 0 1",
            "FISH_PATH_STYLE 1 -1 0 1",
        ] {
            assert!(parse_style(invalid.as_bytes()).is_none(), "{invalid}");
        }
    }
}

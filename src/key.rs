//! Segment, key and tag validation, lookup-argument parsing, path cleaning, and `~` handling.

use std::borrow::Cow;
use std::env;
use std::ffi::{OsStr, OsString};
use std::os::unix::ffi::OsStrExt;
use std::path::PathBuf;

use crate::Error;

/// Keys may have at most this many segments. Lookup, `ls` and completion work at any depth.
pub const MAX_DEPTH: usize = 2;

const NAME_RULES: &str = "allowed: A-Z a-z 0-9 . _ -, not starting with '-'";

fn name_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-')
}

/// A segment is `[A-Za-z0-9._-]+`, must not start with `-`, and must not be only dots.
pub fn valid_segment(s: &[u8]) -> bool {
    !s.is_empty()
        && s[0] != b'-'
        && !s.iter().all(|&b| b == b'.')
        && s.iter().all(|&b| name_byte(b))
}

/// A key is one or more valid segments joined by `/`. Depth is not checked here.
pub fn valid_key(k: &[u8]) -> bool {
    k.split(|&b| b == b'/').all(valid_segment)
}

pub fn depth(k: &[u8]) -> usize {
    k.split(|&b| b == b'/').count()
}

/// A tag as it may appear in the file: `[A-Za-z0-9._-]+`, not starting with `-`
/// (uppercase is tolerated on read).
pub fn valid_tag(t: &[u8]) -> bool {
    !t.is_empty() && t[0] != b'-' && t.iter().all(|&b| name_byte(b))
}

/// Whether a stored path is absolute: `/...`, `~`, or `~/...`. `add` only ever stores these.
pub fn is_stored_absolute(p: &[u8]) -> bool {
    p.starts_with(b"/") || p == b"~" || p.starts_with(b"~/")
}

/// Turns lookup arguments (`personal kotlin-app` or `personal/kotlin-app`) into a key.
pub fn key_from_args(args: &[OsString]) -> Result<String, Error> {
    let mut key = String::new();
    for arg in args {
        let s = arg
            .to_str()
            .filter(|s| valid_key(s.as_bytes()))
            .ok_or_else(|| {
                Error::Invalid(format!(
                    "invalid bookmark name '{}' ({NAME_RULES})",
                    arg.to_string_lossy()
                ))
            })?;
        if !key.is_empty() {
            key.push('/');
        }
        key.push_str(s);
    }
    Ok(key)
}

/// A single `/`-joined key argument for management commands.
pub fn key_arg(arg: &OsStr) -> Result<String, Error> {
    arg.to_str()
        .filter(|s| valid_key(s.as_bytes()))
        .map(str::to_owned)
        .ok_or_else(|| {
            Error::Invalid(format!(
                "invalid key '{}' (expected <namespace>/<name> or <name>; {NAME_RULES})",
                arg.to_string_lossy()
            ))
        })
}

/// Parses a `-t` value: comma-separated, lowercased, deduplicated in order.
pub fn parse_tags(arg: &OsStr) -> Result<Vec<String>, Error> {
    let s = arg
        .to_str()
        .ok_or_else(|| Error::Invalid("tags must be valid UTF-8".into()))?;
    if s.is_empty() {
        return Err(Error::Invalid("empty tag list".into()));
    }
    let mut tags: Vec<String> = Vec::new();
    for t in s.split(',') {
        if t.is_empty() {
            return Err(Error::Invalid(format!("empty tag in '{s}'")));
        }
        if !valid_tag(t.as_bytes()) {
            return Err(Error::Invalid(format!(
                "invalid tag '{t}' (allowed: A-Z a-z 0-9 . _ -, not starting with '-')"
            )));
        }
        let t = t.to_ascii_lowercase();
        if !tags.contains(&t) {
            tags.push(t);
        }
    }
    Ok(tags)
}

/// `$HOME` with trailing slashes removed (a bare `/` is kept). Unset, empty or relative is an
/// error.
pub fn home() -> Result<PathBuf, Error> {
    let raw = env::var_os("HOME")
        .filter(|h| !h.is_empty())
        .ok_or_else(|| Error::Config("HOME is not set".into()))?;
    if !raw.as_bytes().starts_with(b"/") {
        return Err(Error::Config(format!(
            "HOME must be an absolute path (got '{}')",
            raw.to_string_lossy()
        )));
    }
    let mut bytes = raw.as_bytes();
    while bytes.len() > 1 && bytes.ends_with(b"/") {
        bytes = &bytes[..bytes.len() - 1];
    }
    Ok(PathBuf::from(OsStr::from_bytes(bytes)))
}

/// Expands an exact `~` or a leading `~/`. `~user` and everything else is returned unchanged.
pub fn expand<'a>(path: &'a [u8], home: &[u8]) -> Cow<'a, [u8]> {
    let base: &[u8] = if home == b"/" { b"" } else { home };
    if path == b"~" {
        Cow::Owned(if base.is_empty() {
            b"/".to_vec()
        } else {
            base.to_vec()
        })
    } else if let Some(rest) = path.strip_prefix(b"~/") {
        let mut out = Vec::with_capacity(base.len() + 1 + rest.len());
        out.extend_from_slice(base);
        out.push(b'/');
        out.extend_from_slice(rest);
        Cow::Owned(out)
    } else {
        Cow::Borrowed(path)
    }
}

/// Lexically cleans an absolute path: collapses `//`, drops `.`, resolves `..` (clamped at `/`),
/// and strips a trailing `/`. Never touches the filesystem.
pub fn clean(path: &str) -> String {
    let mut parts: Vec<&str> = Vec::new();
    for c in path.split('/') {
        match c {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            c => parts.push(c),
        }
    }
    if parts.is_empty() {
        "/".into()
    } else {
        format!("/{}", parts.join("/"))
    }
}

/// Rewrites a `$HOME` prefix to `~`, only on a path-component boundary.
pub fn contract(path: &str, home: &str) -> String {
    let home = clean(home);
    if home == "/" {
        return path.into();
    }
    match path.strip_prefix(home.as_str()) {
        Some("") => "~".into(),
        Some(rest) if rest.starts_with('/') => format!("~{rest}"),
        _ => path.into(),
    }
}

/// Normalizes a path given to `add` into its stored form.
pub fn normalize_path(arg: &OsStr, home: &str) -> Result<String, Error> {
    let s = arg
        .to_str()
        .ok_or_else(|| Error::Invalid("path is not valid UTF-8".into()))?;
    if s.is_empty() {
        return Err(Error::Invalid("empty path".into()));
    }
    let expanded = match s.strip_prefix('~') {
        Some(rest) if rest.is_empty() || rest.starts_with('/') => format!("{home}{rest}"),
        _ => s.to_owned(),
    };
    let absolute = if expanded.starts_with('/') {
        expanded
    } else {
        format!("{}/{expanded}", current_dir()?)
    };
    // Checked on the final path: $PWD can bring control characters in too.
    let stored = contract(&clean(&absolute), home);
    if stored.bytes().any(|b| b.is_ascii_control()) {
        return Err(Error::Invalid(
            "path contains a tab, newline or other control character".into(),
        ));
    }
    Ok(stored)
}

/// `$PWD` when it is absolute and names the current directory (keeps the shell's logical
/// path through symlinks), else the physical cwd. `$PWD` goes stale when a parent process
/// changes directory without updating it (`env -C`, `make -C`), so it is checked against `.`.
fn current_dir() -> Result<String, Error> {
    use std::os::unix::fs::MetadataExt;
    let same =
        |a: &std::fs::Metadata, b: &std::fs::Metadata| a.dev() == b.dev() && a.ino() == b.ino();
    if let Some(pwd) = env::var("PWD").ok().filter(|p| p.starts_with('/')) {
        if let (Ok(a), Ok(b)) = (std::fs::metadata(&pwd), std::fs::metadata(".")) {
            if same(&a, &b) {
                return Ok(pwd);
            }
        }
    }
    let cwd = env::current_dir()
        .map_err(|e| Error::Config(format!("cannot determine the current directory: {e}")))?;
    cwd.into_os_string()
        .into_string()
        .map_err(|_| Error::Invalid("current directory is not valid UTF-8".into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn segments() {
        for ok in ["personal", "kotlin-app", "a.b_c-d", "A1", ".hidden", "x."] {
            assert!(valid_segment(ok.as_bytes()), "{ok}");
        }
        for bad in ["", "-x", ".", "..", "...", "a b", "a/b", "a\tb", "é", "a!"] {
            assert!(!valid_segment(bad.as_bytes()), "{bad:?}");
        }
    }

    #[test]
    fn keys() {
        assert!(valid_key(b"personal/kotlin-app"));
        assert!(valid_key(b"dotfiles"));
        assert!(valid_key(b"a/b/c"));
        for bad in ["", "/", "a/", "/a", "a//b", "a/../b", "a/-b"] {
            assert!(!valid_key(bad.as_bytes()), "{bad:?}");
        }
        assert_eq!(depth(b"a"), 1);
        assert_eq!(depth(b"a/b/c"), 3);
    }

    #[test]
    fn args_to_key() {
        let k = |a: &[&str]| key_from_args(&a.iter().map(OsString::from).collect::<Vec<_>>());
        assert_eq!(
            k(&["personal", "app"]).ok().as_deref(),
            Some("personal/app")
        );
        assert_eq!(k(&["personal/app"]).ok().as_deref(), Some("personal/app"));
        assert_eq!(k(&["a/b", "c"]).ok().as_deref(), Some("a/b/c"));
        assert!(k(&["personal/"]).is_err());
        assert!(k(&["bad!"]).is_err());
        assert!(k(&[""]).is_err());
    }

    #[test]
    fn tags() {
        let t = |s: &str| parse_tags(OsStr::new(s)).ok();
        assert_eq!(
            t("Docker,infra,docker"),
            Some(vec!["docker".into(), "infra".into()])
        );
        assert_eq!(t("a"), Some(vec!["a".into()]));
        assert!(t("").is_none());
        assert!(t("a,,b").is_none());
        assert!(t("a,").is_none());
        assert!(t("a b").is_none());
        assert!(t("-f").is_none());
        assert!(t("a,-x").is_none());
        assert!(valid_tag(b"a-b"));
    }

    #[test]
    fn tilde_expansion() {
        let e = |p: &str, h: &str| {
            String::from_utf8(expand(p.as_bytes(), h.as_bytes()).into_owned()).unwrap()
        };
        assert_eq!(e("~", "/Users/one"), "/Users/one");
        assert_eq!(e("~/Dev/x", "/Users/one"), "/Users/one/Dev/x");
        assert_eq!(e("~/x", "/"), "/x");
        assert_eq!(e("~", "/"), "/");
        assert_eq!(e("~user/x", "/Users/one"), "~user/x");
        assert_eq!(e("/abs/~/x", "/Users/one"), "/abs/~/x");
        assert_eq!(e("rel", "/Users/one"), "rel");
    }

    #[test]
    fn cleaning() {
        assert_eq!(clean("/a//b/./c/"), "/a/b/c");
        assert_eq!(clean("/a/b/../c"), "/a/c");
        assert_eq!(clean("/../../x"), "/x");
        assert_eq!(clean("/"), "/");
        assert_eq!(clean("/a/.."), "/");
    }

    #[test]
    fn contraction() {
        assert_eq!(contract("/Users/one/x", "/Users/one"), "~/x");
        assert_eq!(contract("/Users/one", "/Users/one"), "~");
        assert_eq!(contract("/Users/one2/x", "/Users/one"), "/Users/one2/x");
        assert_eq!(contract("/Users/one/x", "/Users//one/"), "~/x");
        assert_eq!(contract("/x", "/"), "/x");
    }

    #[test]
    fn normalize_rejects_control_characters_from_any_source() {
        assert!(normalize_path(OsStr::new("/a\tb"), "/Users/one").is_err());
        assert!(normalize_path(OsStr::new("/a\x07b"), "/Users/one").is_err());
        // $HOME itself is contracted back to `~`, so it never reaches the file.
        let home = "/Users/o\nne";
        assert_eq!(
            normalize_path(OsStr::new("~/x"), home).ok().as_deref(),
            Some("~/x")
        );
    }

    #[test]
    fn normalize_absolute_and_tilde() {
        let n = |p: &str| normalize_path(OsStr::new(p), "/Users/one");
        assert_eq!(n("/Users/one/Dev/").ok().as_deref(), Some("~/Dev"));
        assert_eq!(n("~/Dev/../x").ok().as_deref(), Some("~/x"));
        assert_eq!(n("~").ok().as_deref(), Some("~"));
        assert_eq!(n("/opt//w/./a").ok().as_deref(), Some("/opt/w/a"));
        assert!(n("").is_err());
        assert!(n("/a\tb").is_err());
        assert!(n("/a\nb").is_err());
    }
}

//! The bookmarks file: location, the lenient scanner used by reads, strict validation,
//! line-preserving edits, and atomic writes.

mod check;
mod doc;
mod write;

use check::lines_for_write;
pub use check::{check, report, Finding};
pub use doc::{format_line, Doc};
pub use write::write_atomic;

use std::collections::BTreeMap;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use crate::key::{valid_key, valid_tag};
use crate::Error;

const BOM: &[u8] = b"\xEF\xBB\xBF";

/// Header written when bmk creates the file.
pub const HEADER: &[u8] = b"# key<TAB>path<TAB>tags (optional)\n";

/// The bookmarks file always lives at `$HOME/.config/bmk/bookmarks`.
pub fn config_path(home: &Path) -> PathBuf {
    home.join(".config").join("bmk").join("bookmarks")
}

/// Reads the whole file. A missing file is an empty store.
pub fn read(path: &Path) -> Result<Vec<u8>, Error> {
    match fs::read(path) {
        Ok(buf) => Ok(buf),
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(Vec::new()),
        Err(e) => Err(Error::Config(format!(
            "cannot read {}: {e}",
            path.display()
        ))),
    }
}

/// One well-formed bookmark line. `no` is 1-based.
pub struct Line<'a> {
    pub no: usize,
    pub key: &'a [u8],
    pub path: &'a [u8],
    pub tags: &'a [u8],
}

impl Line<'_> {
    pub fn tag_list(&self) -> impl Iterator<Item = &[u8]> {
        self.tags.split(|&b| b == b',').filter(|t| !t.is_empty())
    }
}

enum Kind<'a> {
    Skip,
    Entry(Line<'a>),
    Bad(String),
}

/// Raw lines with 1-based numbers. A trailing newline yields a final empty line, which is skipped.
fn raw_lines(buf: &[u8]) -> impl Iterator<Item = (usize, &[u8])> {
    buf.split(|&b| b == b'\n')
        .enumerate()
        .map(|(i, l)| (i + 1, l))
}

fn classify(no: usize, raw: &[u8]) -> Kind<'_> {
    let mut line = raw.strip_suffix(b"\r").unwrap_or(raw);
    if no == 1 {
        line = line.strip_prefix(BOM).unwrap_or(line);
    }
    if line.first().map_or(true, |&b| b == b'#') || line.iter().all(u8::is_ascii_whitespace) {
        return Kind::Skip;
    }
    let mut fields = line.split(|&b| b == b'\t');
    let key = fields.next().unwrap_or_default();
    let Some(path) = fields.next() else {
        return Kind::Bad("missing tab (expected key<TAB>path[<TAB>tags])".into());
    };
    let tags = fields.next().unwrap_or_default();
    if fields.next().is_some() {
        return Kind::Bad("too many fields (expected key<TAB>path[<TAB>tags])".into());
    }
    if !valid_key(key) {
        return Kind::Bad(format!("invalid key '{}'", String::from_utf8_lossy(key)));
    }
    if path.is_empty() {
        return Kind::Bad("empty path".into());
    }
    Kind::Entry(Line {
        no,
        key,
        path,
        tags,
    })
}

/// Lenient iterator over well-formed lines: malformed lines, comments and blanks are skipped.
pub fn entries(buf: &[u8]) -> impl Iterator<Item = Line<'_>> {
    raw_lines(buf).filter_map(|(no, raw)| match classify(no, raw) {
        Kind::Entry(line) => Some(line),
        _ => None,
    })
}

pub enum Found<'a> {
    Path(&'a [u8]),
    /// The key is a namespace; holds its direct children in file order.
    Namespace(Vec<&'a [u8]>),
    Missing,
}

/// First exact match wins. On a miss, the same pass tells a namespace from an unknown key.
pub fn lookup<'a>(buf: &'a [u8], key: &[u8]) -> Found<'a> {
    let mut children: Vec<&[u8]> = Vec::new();
    for line in entries(buf) {
        if line.key == key {
            return Found::Path(line.path);
        }
        if let Some(child) = child_of(line.key, key) {
            if !children.contains(&child) {
                children.push(child);
            }
        }
    }
    if children.is_empty() {
        Found::Missing
    } else {
        Found::Namespace(children)
    }
}

/// The direct child segment of `parent` that `key` lies under, if any.
pub fn child_of<'a>(key: &'a [u8], parent: &[u8]) -> Option<&'a [u8]> {
    let rest = key.strip_prefix(parent)?.strip_prefix(b"/")?;
    rest.split(|&b| b == b'/').next()
}

/// Whether `key` equals `prefix` or lies below it (segment boundary).
pub fn under(key: &[u8], prefix: &[u8]) -> bool {
    key == prefix || child_of(key, prefix).is_some()
}

/// Proper prefixes of a key: `a/b/c` gives `a`, `a/b`.
fn ancestors(key: &[u8]) -> impl Iterator<Item = &[u8]> {
    key.iter()
        .enumerate()
        .filter(|&(_, &b)| b == b'/')
        .map(move |(i, _)| &key[..i])
}

/// The only way commands change the file: read it, validate it once (refusing on blocking
/// findings), let `edit` change lines in place, then replace the file atomically.
/// Nothing is written if `edit` returns an error.
pub fn update<T>(
    file: &Path,
    edit: impl FnOnce(&[Line<'_>], &mut Doc<'_>) -> Result<T, Error>,
) -> Result<T, Error> {
    let buf = read(file)?;
    let lines = lines_for_write(file, &buf)?;
    let mut doc = Doc::new(&buf);
    let out = edit(&lines, &mut doc)?;
    write_atomic(file, &doc.to_bytes())?;
    Ok(out)
}

/// Valid tags (lowercased) and how many bookmarks use each, sorted by tag. A tag repeated on
/// one line counts once. Shared by `bmk --cli tags` and tag completion.
pub fn tag_counts(lines: &[Line<'_>]) -> BTreeMap<String, usize> {
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for line in lines {
        let mut seen: Vec<String> = Vec::new();
        for tag in line.tag_list().filter(|t| valid_tag(t)) {
            let tag = String::from_utf8_lossy(tag).to_ascii_lowercase();
            if !seen.contains(&tag) {
                *counts.entry(tag.clone()).or_default() += 1;
                seen.push(tag);
            }
        }
    }
    counts
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keys(buf: &[u8]) -> Vec<String> {
        entries(buf)
            .map(|l| String::from_utf8_lossy(l.key).into_owned())
            .collect()
    }

    #[test]
    fn skips_malformed_lines() {
        let buf = b"\xEF\xBB\xBFa/b\t/p\r\n# c\n\n   \nspaces only /x\nbad!\t/x\nk\t\nf\t/p\tt\textra\nok\t/p\ttag\n-x\t/p\nlast\t/p";
        assert_eq!(keys(buf), ["a/b", "ok", "last"]);
    }

    #[test]
    fn lookup_results() {
        let buf = b"p/a\t/1\np/b\t/2\np/a\t/dup\nq/c/d\t/3\n";
        assert!(matches!(lookup(buf, b"p/a"), Found::Path(b"/1")));
        assert!(matches!(lookup(buf, b"p/x"), Found::Missing));
        assert!(matches!(lookup(buf, b"pp"), Found::Missing));
        match lookup(buf, b"p") {
            Found::Namespace(c) => assert_eq!(c, [b"a".as_slice(), b"b"]),
            _ => panic!("expected namespace"),
        }
        match lookup(buf, b"q") {
            Found::Namespace(c) => assert_eq!(c, [b"c".as_slice()]),
            _ => panic!("expected namespace"),
        }
    }
}

//! Strict validation. Writes refuse on blocking findings; `doctor` and `edit` report all.

use std::collections::HashMap;
use std::path::Path;

use super::{ancestors, classify, raw_lines, Kind, Line};
use crate::key::{depth, is_stored_absolute, valid_tag, MAX_DEPTH};
use crate::Error;

pub struct Finding {
    pub no: usize,
    pub msg: String,
    /// Blocking findings make writes refuse; the rest are reported by `doctor` and `edit` only.
    pub blocking: bool,
}

/// Validates the whole file. Returns the well-formed lines and every finding, ordered by line.
pub fn check(buf: &[u8]) -> (Vec<Line<'_>>, Vec<Finding>) {
    let mut lines = Vec::new();
    let mut findings = Vec::new();
    let mut add = |no, msg: String, blocking| findings.push(Finding { no, msg, blocking });

    for (no, raw) in raw_lines(buf) {
        match classify(no, raw) {
            Kind::Skip => {}
            Kind::Bad(msg) => add(no, msg, true),
            Kind::Entry(line) => {
                let shown = || String::from_utf8_lossy(line.key).into_owned();
                if !is_stored_absolute(line.path) {
                    let msg = format!(
                        "path is not absolute: '{}' (use /... or ~/...)",
                        String::from_utf8_lossy(line.path)
                    );
                    add(no, msg, false);
                }
                if line.path.iter().any(u8::is_ascii_control) {
                    let msg = format!(
                        "path contains a control character: {:?}",
                        String::from_utf8_lossy(line.path)
                    );
                    add(no, msg, false);
                }
                if depth(line.key) > MAX_DEPTH {
                    let msg = format!("key '{}' has more than {MAX_DEPTH} segments", shown());
                    add(no, msg, false);
                }
                if !line.tags.is_empty() {
                    let tags_shown = String::from_utf8_lossy(line.tags);
                    let mut seen: Vec<Vec<u8>> = Vec::new();
                    for tag in line.tags.split(|&b| b == b',') {
                        let t = String::from_utf8_lossy(tag);
                        if tag.is_empty() {
                            add(no, format!("empty tag in '{tags_shown}'"), true);
                        } else if !valid_tag(tag) {
                            add(no, format!("invalid tag '{t}'"), true);
                        } else {
                            if tag.iter().any(u8::is_ascii_uppercase) {
                                add(
                                    no,
                                    format!("uppercase tag '{t}' (tags are lowercase)"),
                                    false,
                                );
                            }
                            let lower = tag.to_ascii_lowercase();
                            if seen.contains(&lower) {
                                add(no, format!("duplicate tag '{t}'"), false);
                            } else {
                                seen.push(lower);
                            }
                        }
                    }
                }
                lines.push(line);
            }
        }
    }

    let mut first: HashMap<&[u8], usize> = HashMap::new();
    for line in &lines {
        let key = String::from_utf8_lossy(line.key);
        match first.get(line.key) {
            Some(&n) => add(
                line.no,
                format!("duplicate key '{key}' (first on line {n})"),
                true,
            ),
            None => {
                first.insert(line.key, line.no);
            }
        }
    }
    for line in &lines {
        for parent in ancestors(line.key) {
            if let Some(&n) = first.get(parent) {
                let msg = format!(
                    "'{}' is under bookmark '{}' (line {n}); a key can't be both a bookmark and a namespace",
                    String::from_utf8_lossy(line.key),
                    String::from_utf8_lossy(parent),
                );
                add(line.no, msg, true);
            }
        }
    }

    findings.sort_by_key(|f| f.no);
    (lines, findings)
}

/// Formats findings as `<file>:<line>: <reason>` lines.
pub fn report(file: &Path, findings: &[Finding]) -> String {
    use std::fmt::Write as _;
    let mut out = String::new();
    for f in findings {
        let _ = writeln!(out, "{}:{}: {}", file.display(), f.no, f.msg);
    }
    out
}

/// Strictly validates the file's contents before a write, in a single pass. Returns its
/// bookmark lines, or the blocking findings as an error.
pub(super) fn lines_for_write<'a>(file: &Path, buf: &'a [u8]) -> Result<Vec<Line<'a>>, Error> {
    let (lines, findings) = check(buf);
    let blocking: Vec<Finding> = findings.into_iter().filter(|f| f.blocking).collect();
    if blocking.is_empty() {
        Ok(lines)
    } else {
        Err(Error::Blocked(format!(
            "{}bmk: refusing to write; fix the file ('bmk --cli edit') or run 'bmk --cli doctor'\n",
            report(file, &blocking)
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn control_characters_in_paths_are_reported() {
        let (lines, f) = check(b"a\t/x\ry\nb\t/n\x00ul\n");
        assert_eq!(lines.len(), 2);
        let got: Vec<(usize, bool)> = f.iter().map(|f| (f.no, f.blocking)).collect();
        assert_eq!(got, [(1, false), (2, false)]);
    }

    #[test]
    fn strict_findings() {
        let buf = b"a\t/1\na/b\t/2\nx\t/3\nx\t/4\nno-tab\nt\t/5\tA,b,,a\nd/e/f\t/6\n";
        let (_, f) = check(buf);
        let got: Vec<(usize, bool)> = f.iter().map(|f| (f.no, f.blocking)).collect();
        assert_eq!(
            got,
            [
                (2, true),
                (4, true),
                (5, true),
                (6, false),
                (6, true),
                (6, false),
                (7, false)
            ]
        );
    }
}

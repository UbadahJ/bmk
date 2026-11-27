//! The hidden `__complete` protocol behind the fish completions.
//!
//! Input: the partial token under the cursor, then the completed tokens after `bmk`.
//! Output: `candidate<TAB>description` lines (fish filters by prefix), or the single line
//! `__BMK_DIRS__` asking the script to complete directories. Never fails, never writes stderr.

use std::ffi::{OsStr, OsString};

use crate::cli::{spec_for, BUILTIN_COMMANDS, SPECS};
use crate::store::{self, Line};
use crate::{key, write_stdout};

pub const FISH_SCRIPT: &str = include_str!("../completions/bmk.fish");

const DIRS: &str = "__BMK_DIRS__";

type Candidates = Vec<(String, String)>;

const TOP_FLAGS: &[(&str, &str)] = &[
    ("--cli", "Manage bookmarks"),
    ("-c", "Manage bookmarks"),
    ("--help", "Show help"),
    ("-h", "Show help"),
    ("--version", "Show version"),
    ("-V", "Show version"),
];

const HELP: &[(&str, &str)] = &[("--help", "Show help"), ("-h", "Show help")];
const TAGS: &[(&str, &str)] = &[
    ("--tags", "Comma-separated tags"),
    ("-t", "Comma-separated tags"),
];
const FORCE: &[(&str, &str)] = &[
    ("--force", "Replace an existing bookmark"),
    ("-f", "Replace an existing bookmark"),
];
const CLEAR: &[(&str, &str)] = &[("--clear", "Remove all tags")];

pub fn run(current: &OsStr, tokens: &[OsString]) {
    let current = current.to_string_lossy();
    let tokens: Vec<String> = tokens
        .iter()
        .map(|t| t.to_string_lossy().into_owned())
        .collect();
    let buf = key::home()
        .ok()
        .and_then(|home| store::read(&store::config_path(&home)).ok())
        .unwrap_or_default();
    let lines: Vec<Line<'_>> = store::entries(&buf).collect();

    let mut out = String::new();
    for (candidate, description) in candidates(&current, &tokens, &lines) {
        out.push_str(&candidate);
        if !description.is_empty() {
            out.push('\t');
            out.push_str(&description);
        }
        out.push('\n');
    }
    let _ = write_stdout(out.as_bytes());
}

fn candidates(current: &str, tokens: &[String], lines: &[Line<'_>]) -> Candidates {
    match tokens.first().map(String::as_str) {
        Some("-c" | "--cli") => management(current, &tokens[1..], lines),
        _ => lookup(current, tokens, lines),
    }
}

fn fixed(list: &[&[(&str, &str)]]) -> Candidates {
    list.iter()
        .flat_map(|l| l.iter())
        .map(|&(c, d)| (c.to_owned(), d.to_owned()))
        .collect()
}

fn lossy(b: &[u8]) -> String {
    String::from_utf8_lossy(b).into_owned()
}

/// Lookup mode: children of the segments typed so far.
fn lookup(current: &str, tokens: &[String], lines: &[Line<'_>]) -> Candidates {
    if tokens.is_empty() && current.starts_with('-') {
        return fixed(&[TOP_FLAGS]);
    }
    if tokens.iter().any(|t| t.starts_with('-')) {
        return Vec::new();
    }
    let prefix = tokens.join("/");
    if !prefix.is_empty() && !key::valid_key(prefix.as_bytes()) {
        return Vec::new();
    }
    if current.contains('/') {
        // Don't offer the namespace the user has already typed in full.
        below(lines, &prefix)
            .into_iter()
            .filter(|(c, _)| !(c.ends_with('/') && c == current))
            .collect()
    } else {
        children(lines, &prefix)
    }
}

/// Direct children of `prefix` (top level when empty), described by path or as a namespace.
fn children(lines: &[Line<'_>], prefix: &str) -> Candidates {
    let mut out: Candidates = Vec::new();
    for line in lines {
        let child = if prefix.is_empty() {
            line.key.split(|&b| b == b'/').next()
        } else {
            store::child_of(line.key, prefix.as_bytes())
        };
        let Some(child) = child else { continue };
        let name = lossy(child);
        if out.iter().any(|(c, _)| *c == name) {
            continue;
        }
        let full_len = if prefix.is_empty() {
            child.len()
        } else {
            prefix.len() + 1 + child.len()
        };
        let description = if line.key.len() == full_len {
            lossy(line.path)
        } else {
            "namespace".to_owned()
        };
        out.push((name, description));
    }
    out
}

/// Every bookmark key and namespace prefix below `prefix`, relative to it (for `a/b` typing).
fn below(lines: &[Line<'_>], prefix: &str) -> Candidates {
    let strip = |k: &[u8]| -> Option<String> {
        if prefix.is_empty() {
            Some(lossy(k))
        } else {
            k.strip_prefix(prefix.as_bytes())?
                .strip_prefix(b"/")
                .map(lossy)
        }
    };
    let mut out: Candidates = Vec::new();
    for (candidate, description) in namespaces(lines).into_iter().chain(bookmarks(lines)) {
        if let Some(rel) = strip(candidate.as_bytes()).filter(|r| !r.is_empty()) {
            out.push((rel, description));
        }
    }
    out
}

/// Every bookmark key, described by its path.
fn bookmarks(lines: &[Line<'_>]) -> Candidates {
    lines
        .iter()
        .map(|l| (lossy(l.key), lossy(l.path)))
        .collect()
}

/// Every namespace as `ns/`, in first-seen order.
fn namespaces(lines: &[Line<'_>]) -> Candidates {
    let mut out: Candidates = Vec::new();
    for line in lines {
        for (i, _) in line.key.iter().enumerate().filter(|&(_, &b)| b == b'/') {
            let ns = format!("{}/", lossy(&line.key[..i]));
            if !out.iter().any(|(c, _)| *c == ns) {
                out.push((ns, "namespace".to_owned()));
            }
        }
    }
    out
}

fn management(current: &str, rest: &[String], lines: &[Line<'_>]) -> Candidates {
    let Some((cmd, args)) = rest.split_first() else {
        return commands(true);
    };
    let spec = spec_for(cmd);
    let takes_tags = spec.is_some_and(|s| s.tags);

    let mut only_positionals = false;
    let mut awaiting_tags = false;
    let mut positionals = 0;
    for arg in args {
        if awaiting_tags {
            awaiting_tags = false;
        } else if !only_positionals && arg == "--" {
            only_positionals = true;
        } else if !only_positionals && arg.len() > 1 && arg.starts_with('-') {
            awaiting_tags = takes_tags && (arg == "-t" || arg == "--tags");
        } else {
            positionals += 1;
        }
    }
    if awaiting_tags {
        return tag_values(current, lines);
    }
    if !only_positionals && current.starts_with('-') {
        return match spec {
            Some(s) => {
                let mut flags: Vec<&[(&str, &str)]> = Vec::new();
                for (on, list) in [(s.tags, TAGS), (s.force, FORCE), (s.clear, CLEAR)] {
                    if on {
                        flags.push(list);
                    }
                }
                flags.push(HELP);
                fixed(&flags)
            }
            None => Vec::new(),
        };
    }
    match (cmd.as_str(), positionals) {
        ("add", 0) | ("mv", 1) => namespaces(lines),
        ("add", 1) => vec![(DIRS.to_owned(), String::new())],
        ("rm" | "tag" | "mv", 0) => bookmarks(lines),
        ("help", 0) => commands(false),
        ("ls", 0) => namespaces(lines)
            .into_iter()
            .chain(bookmarks(lines))
            .collect(),
        _ => Vec::new(),
    }
}

/// Command names with summaries, from the parser's own table.
fn commands(with_builtins: bool) -> Candidates {
    let specs = SPECS.iter().map(|s| (s.name, s.summary));
    let builtins = BUILTIN_COMMANDS.iter().copied().filter(|_| with_builtins);
    specs
        .chain(builtins)
        .map(|(c, d)| (c.to_owned(), d.to_owned()))
        .collect()
}

/// Known tags with counts; after `docker,` the candidates are `docker,<tag>`.
fn tag_values(current: &str, lines: &[Line<'_>]) -> Candidates {
    let counts = store::tag_counts(lines);
    let (head, typed): (String, Vec<String>) = match current.rsplit_once(',') {
        Some((head, _)) => (
            format!("{head},"),
            head.split(',').map(str::to_ascii_lowercase).collect(),
        ),
        None => (String::new(), Vec::new()),
    };
    counts
        .into_iter()
        .filter(|(t, _)| !typed.contains(t))
        .map(|(t, n)| {
            let s = if n == 1 { "" } else { "s" };
            (format!("{head}{t}"), format!("{n} bookmark{s}"))
        })
        .collect()
}

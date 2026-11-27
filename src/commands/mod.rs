//! Management commands, one module each, plus the helpers they share.

mod add;
mod doctor;
mod edit;
mod ls;
mod mv;
mod path;
mod rm;
mod tag;

use std::os::unix::ffi::OsStrExt;
use std::path::PathBuf;

use crate::cli::Cmd;
use crate::key::{self, MAX_DEPTH};
use crate::store::{self, Line};
use crate::{write_stdout, Error};

pub fn run(cmd: Cmd) -> Result<(), Error> {
    match cmd {
        Cmd::Print(text) => write_stdout(text.as_bytes()),
        Cmd::Complete { current, tokens } => {
            crate::complete::run(&current, &tokens);
            Ok(())
        }
        Cmd::Add {
            key,
            path,
            tags,
            force,
        } => add::run(&key, &path, &tags, force),
        Cmd::Rm { key } => rm::run(&key),
        Cmd::Mv { from, to } => mv::run(&from, &to),
        Cmd::Tag { key, tags } => tag::run(&key, tags.as_deref()),
        Cmd::Ls { prefix, tags } => ls::run(prefix.as_deref(), &tags),
        Cmd::Tags => ls::tags(),
        Cmd::Edit => edit::run(),
        Cmd::Doctor => doctor::run(),
        Cmd::Path => path::run(),
    }
}

pub(super) struct Ctx {
    pub home: PathBuf,
    pub file: PathBuf,
}

impl Ctx {
    pub fn new() -> Result<Self, Error> {
        let home = key::home()?;
        let file = store::config_path(&home);
        Ok(Ctx { home, file })
    }

    pub fn home_bytes(&self) -> &[u8] {
        self.home.as_os_str().as_bytes()
    }

    pub fn home_str(&self) -> Result<&str, Error> {
        self.home
            .to_str()
            .ok_or_else(|| Error::Config("HOME is not valid UTF-8".into()))
    }
}

pub(super) fn lossy(b: &[u8]) -> String {
    String::from_utf8_lossy(b).into_owned()
}

pub(super) fn find<'b, 'a>(lines: &'b [Line<'a>], key: &str) -> Option<&'b Line<'a>> {
    lines.iter().find(|l| l.key == key.as_bytes())
}

fn children(lines: &[Line<'_>], key: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for l in lines {
        if let Some(c) = store::child_of(l.key, key.as_bytes()) {
            let c = lossy(c);
            if !out.contains(&c) {
                out.push(c);
            }
        }
    }
    out
}

/// The error for a key that isn't a bookmark: a namespace, or simply missing.
pub(super) fn not_a_bookmark(lines: &[Line<'_>], key: &str) -> Error {
    let kids = children(lines, key);
    if kids.is_empty() {
        Error::NotFound(key.into())
    } else {
        Error::IsNamespace(key.into(), kids)
    }
}

/// Checks that `key` can be a bookmark, ignoring the bookmark at `except` (for renames).
pub(super) fn check_new_key(
    lines: &[Line<'_>],
    key: &str,
    except: Option<&str>,
) -> Result<(), Error> {
    if key::depth(key.as_bytes()) > MAX_DEPTH {
        return Err(Error::Conflict(format!(
            "'{key}' is too deep (at most {MAX_DEPTH} segments)"
        )));
    }
    let others = || {
        lines
            .iter()
            .filter(|l| Some(l.key) != except.map(str::as_bytes))
    };
    if let Some(parent) = others().find(|l| store::child_of(key.as_bytes(), l.key).is_some()) {
        return Err(Error::Conflict(format!(
            "'{}' is a bookmark, so it can't be a namespace",
            lossy(parent.key)
        )));
    }
    if others().any(|l| store::child_of(l.key, key.as_bytes()).is_some()) {
        return Err(Error::Conflict(format!(
            "'{key}' is a namespace; pick another name"
        )));
    }
    Ok(())
}

pub(super) fn describe(key: &[u8], path: &[u8], tags: &[u8]) -> String {
    let mut s = format!("{} -> {}", lossy(key), lossy(path));
    if !tags.is_empty() {
        s.push_str(&format!(" [{}]", lossy(tags)));
    }
    s
}

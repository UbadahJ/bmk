//! Management-mode argument parser (`bmk --cli <command> ...`).

pub mod help;

use std::ffi::{OsStr, OsString};
use std::os::unix::ffi::OsStrExt;

use crate::key::{key_arg, parse_tags};
use crate::Error;

pub enum Cmd {
    Add {
        key: String,
        path: OsString,
        tags: Vec<String>,
        force: bool,
    },
    Rm {
        key: String,
    },
    Mv {
        from: String,
        to: String,
    },
    /// `tags: None` means `--clear`.
    Tag {
        key: String,
        tags: Option<Vec<String>>,
    },
    Ls {
        prefix: Option<String>,
        tags: Vec<String>,
    },
    Tags,
    Edit,
    Doctor,
    Path,
    /// Help, version or script text for stdout.
    Print(&'static str),
    /// The hidden completion protocol; tokens are raw data.
    Complete {
        current: OsString,
        tokens: Vec<OsString>,
    },
}

/// A management command: the single source of truth for the parser, `help <command>`, and
/// completion of command names and flags.
pub struct Spec {
    pub name: &'static str,
    /// One line, used as the completion description.
    pub summary: &'static str,
    pub usage: &'static str,
    /// Minimum and maximum number of positional arguments.
    pub positionals: (usize, usize),
    /// Accepts `-t/--tags <tags>`.
    pub tags: bool,
    /// Accepts `-f/--force`.
    pub force: bool,
    /// Accepts `--clear`.
    pub clear: bool,
}

const fn spec(
    name: &'static str,
    summary: &'static str,
    usage: &'static str,
    positionals: (usize, usize),
    (tags, force, clear): (bool, bool, bool),
) -> Spec {
    Spec {
        name,
        summary,
        usage,
        positionals,
        tags,
        force,
        clear,
    }
}

const NO_FLAGS: (bool, bool, bool) = (false, false, false);

/// Every management command that `parse` dispatches through `parse_flags`.
pub const SPECS: &[Spec] = &[
    spec(
        "add",
        "Add a bookmark",
        help::ADD,
        (2, 2),
        (true, true, false),
    ),
    spec("rm", "Remove a bookmark", help::RM, (1, 1), NO_FLAGS),
    spec("mv", "Rename a bookmark", help::MV, (2, 2), NO_FLAGS),
    spec(
        "tag",
        "Replace or clear a bookmark's tags",
        help::TAG,
        (1, 1),
        (true, false, true),
    ),
    spec(
        "ls",
        "List bookmarks",
        help::LS,
        (0, 1),
        (true, false, false),
    ),
    spec(
        "tags",
        "List tags with counts",
        help::TAGS,
        (0, 0),
        NO_FLAGS,
    ),
    spec(
        "edit",
        "Open the bookmarks file in an editor",
        help::EDIT,
        (0, 0),
        NO_FLAGS,
    ),
    spec(
        "doctor",
        "Check the file and every target",
        help::DOCTOR,
        (0, 0),
        NO_FLAGS,
    ),
    spec(
        "path",
        "Print the bookmarks file location",
        help::PATH,
        (0, 0),
        NO_FLAGS,
    ),
    spec(
        "fish",
        "Print the fish completion script",
        help::FISH,
        (0, 0),
        NO_FLAGS,
    ),
];

/// Commands handled before `SPECS` (they take no flags), with their summaries.
pub const BUILTIN_COMMANDS: &[(&str, &str)] = &[
    ("help", "Show management help"),
    ("version", "Show version"),
];

pub fn spec_for(name: &str) -> Option<&'static Spec> {
    SPECS.iter().find(|s| s.name == name)
}

#[derive(Default)]
struct Parsed {
    pos: Vec<OsString>,
    tags: Option<Vec<String>>,
    force: bool,
    clear: bool,
}

fn usage(msg: String, cmd: Option<&str>) -> Error {
    let hint = match cmd {
        Some(c) => format!("Try 'bmk --cli {c} --help'."),
        None => "Try 'bmk --cli help'.".into(),
    };
    Error::Usage(format!("{msg}\n{hint}"))
}

/// Parses everything after `--cli`.
pub fn parse(args: &[OsString]) -> Result<Cmd, Error> {
    let Some((name, rest)) = args.split_first() else {
        return Err(Error::UsageText(help::CLI));
    };
    let name = name.to_str().unwrap_or_default();
    match name {
        "__complete" => {
            let current = rest.first().cloned().unwrap_or_default();
            let tokens = rest.get(1..).unwrap_or_default().to_vec();
            return Ok(Cmd::Complete { current, tokens });
        }
        "help" | "-h" | "--help" => return help_for(rest),
        "version" | "-V" | "--version" => {
            return match rest.first() {
                None => Ok(Cmd::Print(help::VERSION)),
                Some(extra) => Err(usage(
                    format!("unexpected argument '{}'", extra.to_string_lossy()),
                    None,
                )),
            }
        }
        _ => {}
    }
    let Some(spec) = spec_for(name) else {
        return Err(usage(format!("unknown command '{name}'"), None));
    };
    let Some(p) = parse_flags(spec, rest)? else {
        return Ok(Cmd::Print(spec.usage));
    };

    let mut pos = p.pos.iter().map(OsString::as_os_str);
    let mut next = || pos.next().unwrap_or_default();
    Ok(match spec.name {
        "add" => Cmd::Add {
            key: key_arg(next())?,
            path: next().to_owned(),
            tags: p.tags.unwrap_or_default(),
            force: p.force,
        },
        "rm" => Cmd::Rm {
            key: key_arg(next())?,
        },
        "mv" => Cmd::Mv {
            from: key_arg(next())?,
            to: key_arg(next())?,
        },
        "tag" => {
            let key = key_arg(next())?;
            match (p.tags, p.clear) {
                (Some(tags), false) => Cmd::Tag {
                    key,
                    tags: Some(tags),
                },
                (None, true) => Cmd::Tag { key, tags: None },
                _ => {
                    return Err(usage(
                        "give either -t <tags> or --clear".into(),
                        Some("tag"),
                    ))
                }
            }
        }
        "ls" => Cmd::Ls {
            // Completion offers namespaces as `ns/`, so accept one trailing slash here.
            prefix: p
                .pos
                .first()
                .map(|a| {
                    let b = a.as_bytes();
                    key_arg(OsStr::from_bytes(b.strip_suffix(b"/").unwrap_or(b)))
                })
                .transpose()?,
            tags: p.tags.unwrap_or_default(),
        },
        "tags" => Cmd::Tags,
        "edit" => Cmd::Edit,
        "doctor" => Cmd::Doctor,
        "fish" => Cmd::Print(crate::complete::FISH_SCRIPT),
        _ => Cmd::Path,
    })
}

/// `bmk --cli help [<command>]`.
fn help_for(rest: &[OsString]) -> Result<Cmd, Error> {
    match rest {
        [] => Ok(Cmd::Print(help::CLI)),
        [cmd] => match cmd.to_str().unwrap_or_default() {
            "help" | "version" => Ok(Cmd::Print(help::CLI)),
            name => spec_for(name)
                .map(|s| Cmd::Print(s.usage))
                .ok_or_else(|| usage(format!("unknown command '{name}'"), None)),
        },
        [_, extra, ..] => Err(usage(
            format!("unexpected argument '{}'", extra.to_string_lossy()),
            None,
        )),
    }
}

/// Returns `None` when `-h`/`--help` was given.
fn parse_flags(spec: &Spec, args: &[OsString]) -> Result<Option<Parsed>, Error> {
    let mut p = Parsed::default();
    let mut it = args.iter();
    let mut only_positionals = false;
    while let Some(arg) = it.next() {
        let is_flag = !only_positionals && arg.len() > 1 && arg.as_bytes().starts_with(b"-");
        if !is_flag {
            p.pos.push(arg.clone());
            continue;
        }
        match arg.to_str() {
            Some("--") => only_positionals = true,
            Some("-h" | "--help") => return Ok(None),
            Some(flag @ ("-t" | "--tags")) if spec.tags => {
                // A value that looks like a flag (`-t -f`) means the value was forgotten.
                let value = it
                    .next()
                    .filter(|v| !(v.len() > 1 && v.as_bytes().starts_with(b"-")))
                    .ok_or_else(|| {
                        usage(format!("option '{flag}' needs a value"), Some(spec.name))
                    })?;
                if p.tags.is_some() {
                    return Err(usage("tags given more than once".into(), Some(spec.name)));
                }
                p.tags = Some(parse_tags(value)?);
            }
            Some("-f" | "--force") if spec.force => p.force = true,
            Some("--clear") if spec.clear => p.clear = true,
            _ => {
                return Err(usage(
                    format!(
                        "unknown option '{}' for '{}'",
                        arg.to_string_lossy(),
                        spec.name
                    ),
                    Some(spec.name),
                ))
            }
        }
    }
    let (min, max) = spec.positionals;
    if p.pos.len() < min {
        return Err(usage("missing argument".into(), Some(spec.name)));
    }
    if p.pos.len() > max {
        let extra = p.pos[max].to_string_lossy().into_owned();
        return Err(usage(
            format!("unexpected argument '{extra}'"),
            Some(spec.name),
        ));
    }
    Ok(Some(p))
}

/// Whether an argument switches to management mode.
pub fn is_cli_switch(arg: &OsStr) -> bool {
    matches!(arg.to_str(), Some("-c" | "--cli"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_command_is_in_the_overview() {
        for s in SPECS {
            let line = format!("\n  {} ", s.name);
            assert!(
                help::CLI.contains(&line),
                "{} missing from help::CLI",
                s.name
            );
        }
        for (name, _) in BUILTIN_COMMANDS {
            assert!(help::CLI.contains(name), "{name} missing from help::CLI");
        }
    }

    #[test]
    fn every_spec_has_matching_usage() {
        for s in SPECS {
            assert!(
                s.usage.starts_with(&format!("Usage: bmk --cli {}", s.name)),
                "{}",
                s.name
            );
        }
    }
}

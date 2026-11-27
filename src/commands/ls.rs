//! `bmk --cli ls`.

use std::io::{self, IsTerminal};

use super::{lossy, Ctx};
use crate::store::{self, Line};
use crate::{key, write_stdout, Error};

fn has_all_tags(line: &Line<'_>, wanted: &[String]) -> bool {
    wanted.iter().all(|w| {
        line.tag_list()
            .any(|t| t.eq_ignore_ascii_case(w.as_bytes()))
    })
}

/// Rows as aligned columns (TTY) or tab-separated lines (piped).
fn render(rows: &[Vec<String>], tty: bool) -> Vec<u8> {
    let mut out = String::new();
    if tty {
        let cols = rows.iter().map(Vec::len).max().unwrap_or(0);
        let widths: Vec<usize> = (0..cols)
            .map(|c| {
                rows.iter()
                    .filter_map(|r| r.get(c))
                    .map(|s| s.chars().count())
                    .max()
                    .unwrap_or(0)
            })
            .collect();
        for row in rows {
            let mut line = String::new();
            for (c, cell) in row.iter().enumerate() {
                line.push_str(cell);
                if c + 1 < row.len() {
                    let pad = widths[c] - cell.chars().count() + 2;
                    line.extend(std::iter::repeat(' ').take(pad));
                }
            }
            out.push_str(line.trim_end());
            out.push('\n');
        }
    } else {
        for row in rows {
            out.push_str(&row.join("\t"));
            out.push('\n');
        }
    }
    out.into_bytes()
}

pub(super) fn run(prefix: Option<&str>, tags: &[String]) -> Result<(), Error> {
    let ctx = Ctx::new()?;
    let buf = store::read(&ctx.file)?;
    let tty = io::stdout().is_terminal();
    let in_scope: Vec<Line<'_>> = store::entries(&buf)
        .filter(|l| prefix.map_or(true, |p| store::under(l.key, p.as_bytes())))
        .collect();
    if let Some(p) = prefix {
        if in_scope.is_empty() {
            return Err(Error::NoMatches(format!("no bookmarks under '{p}'")));
        }
    }
    let rows: Vec<Vec<String>> = in_scope
        .iter()
        .filter(|l| has_all_tags(l, tags))
        .map(|l| {
            let path = if tty {
                lossy(l.path)
            } else {
                lossy(&key::expand(l.path, ctx.home_bytes()))
            };
            vec![lossy(l.key), path, lossy(l.tags)]
        })
        .collect();
    if rows.is_empty() && !tags.is_empty() {
        return Err(Error::NoMatches(format!(
            "no bookmarks tagged '{}'",
            tags.join(",")
        )));
    }
    write_stdout(&render(&rows, tty))
}

pub(super) fn tags() -> Result<(), Error> {
    let ctx = Ctx::new()?;
    let buf = store::read(&ctx.file)?;
    let lines: Vec<Line<'_>> = store::entries(&buf).collect();
    let rows: Vec<Vec<String>> = store::tag_counts(&lines)
        .into_iter()
        .map(|(t, n)| vec![t, n.to_string()])
        .collect();
    write_stdout(&render(&rows, io::stdout().is_terminal()))
}

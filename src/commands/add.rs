//! `bmk --cli add`.

use std::ffi::OsStr;

use super::{check_new_key, describe, find, Ctx};
use crate::store;
use crate::{key, note, Error};

pub(super) fn run(key: &str, path: &OsStr, tags: &[String], force: bool) -> Result<(), Error> {
    let ctx = Ctx::new()?;
    let stored = key::normalize_path(path, ctx.home_str()?)?;
    let tags = tags.join(",");
    let line = store::format_line(key.as_bytes(), stored.as_bytes(), tags.as_bytes());

    let verb = store::update(&ctx.file, |lines, doc| {
        check_new_key(lines, key, None)?;
        match find(lines, key) {
            Some(existing) if force => doc.replace(existing.no, line).map(|()| "replaced"),
            Some(_) => Err(Error::Conflict(format!(
                "bookmark '{key}' already exists (use -f to replace it)"
            ))),
            None => {
                doc.push(line);
                Ok("added")
            }
        }
    })?;
    note(&format!(
        "bmk: {verb} {}",
        describe(key.as_bytes(), stored.as_bytes(), tags.as_bytes())
    ));
    Ok(())
}

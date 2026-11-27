//! `bmk --cli mv`.

use super::{check_new_key, find, not_a_bookmark, Ctx};
use crate::store;
use crate::{note, Error};

pub(super) fn run(from: &str, to: &str) -> Result<(), Error> {
    let ctx = Ctx::new()?;
    store::update(&ctx.file, |lines, doc| {
        let line = find(lines, from).ok_or_else(|| not_a_bookmark(lines, from))?;
        if from == to {
            return Err(Error::Conflict(format!("'{from}' is already called that")));
        }
        if find(lines, to).is_some() {
            return Err(Error::Conflict(format!("bookmark '{to}' already exists")));
        }
        check_new_key(lines, to, Some(from))?;
        doc.replace(
            line.no,
            store::format_line(to.as_bytes(), line.path, line.tags),
        )
    })?;
    note(&format!("bmk: renamed {from} -> {to}"));
    Ok(())
}

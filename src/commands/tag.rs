//! `bmk --cli tag`.

use super::{describe, find, not_a_bookmark, Ctx};
use crate::store;
use crate::{note, Error};

pub(super) fn run(key: &str, tags: Option<&[String]>) -> Result<(), Error> {
    let ctx = Ctx::new()?;
    let joined = tags.map(|t| t.join(",")).unwrap_or_default();
    let tagged = store::update(&ctx.file, |lines, doc| {
        let line = find(lines, key).ok_or_else(|| not_a_bookmark(lines, key))?;
        doc.replace(
            line.no,
            store::format_line(line.key, line.path, joined.as_bytes()),
        )?;
        Ok(describe(line.key, line.path, joined.as_bytes()))
    })?;
    note(&format!("bmk: tagged {tagged}"));
    Ok(())
}

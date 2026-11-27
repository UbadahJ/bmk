//! `bmk --cli rm`.

use super::{describe, find, not_a_bookmark, Ctx};
use crate::store;
use crate::{note, Error};

pub(super) fn run(key: &str) -> Result<(), Error> {
    let ctx = Ctx::new()?;
    let removed = store::update(&ctx.file, |lines, doc| {
        let line = find(lines, key).ok_or_else(|| not_a_bookmark(lines, key))?;
        doc.remove(line.no)?;
        Ok(describe(line.key, line.path, line.tags))
    })?;
    note(&format!("bmk: removed {removed}"));
    Ok(())
}

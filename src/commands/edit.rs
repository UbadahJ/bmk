//! `bmk --cli edit`.

use std::process::Command;

use super::Ctx;
use crate::store;
use crate::{note, Error};

pub(super) fn run() -> Result<(), Error> {
    let ctx = Ctx::new()?;
    if !ctx.file.exists() {
        store::write_atomic(&ctx.file, store::HEADER)?;
    }
    let editor = ["VISUAL", "EDITOR"]
        .iter()
        .filter_map(|v| std::env::var(v).ok())
        .map(|v| v.trim().to_owned())
        .find(|v| !v.is_empty())
        .unwrap_or_else(|| "vi".into());
    let status = Command::new("sh")
        .arg("-c")
        .arg(format!("{editor} \"$1\""))
        .arg("sh")
        .arg(&ctx.file)
        .status()
        .map_err(|e| Error::Config(format!("cannot run editor '{editor}': {e}")))?;

    // Syntax and schema only: targets are never checked here.
    let buf = store::read(&ctx.file)?;
    let (_, findings) = store::check(&buf);
    if !findings.is_empty() {
        note(store::report(&ctx.file, &findings).trim_end());
    }
    match status.code() {
        Some(0) => Ok(()),
        Some(code) => Err(Error::Status(code)),
        None => Err(Error::Status(1)),
    }
}

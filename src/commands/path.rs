//! `bmk --cli path`.

use std::os::unix::ffi::OsStrExt;

use super::Ctx;
use crate::{note, write_stdout, Error};

pub(super) fn run() -> Result<(), Error> {
    let ctx = Ctx::new()?;
    let mut out = ctx.file.as_os_str().as_bytes().to_vec();
    out.push(b'\n');
    if !ctx.file.exists() {
        note(
            "bmk: note: the file doesn't exist yet; 'bmk --cli add' or 'bmk --cli edit' creates it",
        );
    }
    write_stdout(&out)
}

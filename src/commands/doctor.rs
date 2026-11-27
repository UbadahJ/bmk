//! `bmk --cli doctor`.

use std::ffi::OsStr;
use std::fs;
use std::io::ErrorKind;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;

use super::{lossy, Ctx};
use crate::store;
use crate::{key, note, Error};

pub(super) fn run() -> Result<(), Error> {
    let ctx = Ctx::new()?;
    let buf = store::read(&ctx.file)?;
    let (lines, mut findings) = store::check(&buf);
    for line in &lines {
        if !key::is_stored_absolute(line.path) {
            continue; // reported by `check`; checking it would depend on the current directory
        }
        let expanded = key::expand(line.path, ctx.home_bytes());
        let target = Path::new(OsStr::from_bytes(&expanded));
        let shown = lossy(line.path);
        let problem = match fs::metadata(target) {
            Ok(m) if m.is_dir() => continue,
            Ok(_) => format!("target is not a directory: {shown}"),
            Err(e) if e.kind() == ErrorKind::NotFound => {
                format!("target does not exist: {shown}")
            }
            Err(e) => format!("cannot access target {shown}: {e}"),
        };
        findings.push(store::Finding {
            no: line.no,
            msg: problem,
            blocking: false,
        });
    }
    findings.sort_by_key(|f| f.no);
    if findings.is_empty() {
        let s = if lines.len() == 1 { "" } else { "s" };
        note(&format!(
            "bmk: no problems found ({} bookmark{s})",
            lines.len()
        ));
        return Ok(());
    }
    note(store::report(&ctx.file, &findings).trim_end());
    Err(Error::DoctorFailed(findings.len()))
}

//! Atomic replacement of the bookmarks file.

use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::Error;

static SEQ: AtomicUsize = AtomicUsize::new(0);

/// Copy, update, replace: write a sibling temp file, then rename it over the original.
pub fn write_atomic(file: &Path, bytes: &[u8]) -> Result<(), Error> {
    let fail = |e: io::Error| Error::Config(format!("cannot write {}: {e}", file.display()));
    let dir = file.parent().unwrap_or(Path::new("."));
    fs::create_dir_all(dir).map_err(fail)?;
    let name = file.file_name().unwrap_or_default().to_string_lossy();
    let permissions = fs::metadata(file).ok().map(|m| m.permissions());

    // A name left behind by a dead process with the same PID is skipped, not removed.
    let mut attempt = 0;
    let (tmp, mut f) = loop {
        let seq = SEQ.fetch_add(1, Ordering::Relaxed);
        let tmp = dir.join(format!(".{name}.tmp.{}.{seq}", std::process::id()));
        match OpenOptions::new().write(true).create_new(true).open(&tmp) {
            Ok(f) => break (tmp, f),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists && attempt < 16 => attempt += 1,
            Err(e) => return Err(fail(e)),
        }
    };

    // From here on the temp file is ours, so removing it on failure is safe.
    let result = (|| {
        if let Some(p) = permissions {
            f.set_permissions(p)?;
        }
        f.write_all(bytes)?;
        f.sync_all()?;
        drop(f);
        fs::rename(&tmp, file)
    })();
    if let Err(e) = result {
        let _ = fs::remove_file(&tmp);
        return Err(fail(e));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skips_and_keeps_stale_temp_files() {
        let dir = std::env::temp_dir().join(format!("bmk-write-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let file = dir.join("bookmarks");
        // Occupy the next few temp names, as a dead process with our PID would have.
        let next = SEQ.load(Ordering::Relaxed);
        let stale: Vec<_> = (next..next + 3)
            .map(|s| dir.join(format!(".bookmarks.tmp.{}.{s}", std::process::id())))
            .collect();
        for p in &stale {
            fs::write(p, "stale").unwrap();
        }
        write_atomic(&file, b"new\n").unwrap();
        assert_eq!(fs::read(&file).unwrap(), b"new\n");
        for p in &stale {
            assert_eq!(fs::read(p).unwrap(), b"stale", "must not delete {p:?}");
        }
        fs::remove_dir_all(&dir).unwrap();
    }
}

//! The only ways bmk writes output. `println!`/`eprintln!` panic on write errors, and with
//! `panic = "abort"` that would be a SIGABRT, so nothing else should print.

use std::io::{self, ErrorKind, Write};

use crate::Error;

/// Writes everything to stdout in one go. A closed pipe is a quiet success.
pub fn write_stdout(bytes: &[u8]) -> Result<(), Error> {
    let mut out = io::stdout().lock();
    match out.write_all(bytes).and_then(|()| out.flush()) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == ErrorKind::BrokenPipe => Ok(()),
        Err(e) => Err(Error::Io(e)),
    }
}

/// Writes a line to stderr, ignoring failures (`eprintln!` would panic).
pub fn note(msg: &str) {
    let _ = writeln!(io::stderr(), "{msg}");
}

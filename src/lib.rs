//! bmk: scoped directory bookmarks. `bmk <namespace> <name>` prints a path.
//!
//! The binary in `main.rs` only forwards arguments to [`main`]. Everything else is private.

// With `panic = "abort"`, a panic is a SIGABRT, and `println!`/`eprintln!` panic when a write
// fails. Output goes through `output::{write_stdout, note}` instead. Tests may panic freely.
#![cfg_attr(
    not(test),
    deny(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::todo,
        clippy::unimplemented,
        clippy::print_stdout,
        clippy::print_stderr
    )
)]

mod cli;
mod commands;
mod complete;
mod error;
mod key;
mod lookup;
mod output;
mod store;

use std::ffi::OsString;
use std::io::{self, Write};
use std::os::unix::ffi::OsStrExt;

use cli::help;
pub(crate) use error::Error;
pub(crate) use output::{note, write_stdout};

/// Runs bmk with the arguments after the program name and returns the exit code.
pub fn main(args: &[OsString]) -> i32 {
    match run(args) {
        Ok(()) => 0,
        Err(e) => {
            // Nothing useful to do if stderr itself is gone.
            let _ = io::stderr().write_all(e.message().as_bytes());
            e.exit_code()
        }
    }
}

fn run(args: &[OsString]) -> Result<(), Error> {
    let Some(first) = args.first() else {
        return Err(Error::UsageText(help::LOOKUP));
    };
    if cli::is_cli_switch(first) {
        return commands::run(cli::parse(&args[1..])?);
    }
    match (first.to_str(), args.len()) {
        (Some("-h" | "--help"), 1) => return write_stdout(help::LOOKUP.as_bytes()),
        (Some("-V" | "--version"), 1) => return write_stdout(help::VERSION.as_bytes()),
        _ => {}
    }
    if let Some(flag) = args.iter().find(|a| a.as_bytes().starts_with(b"-")) {
        return Err(Error::Usage(format!(
            "unexpected option '{}'\nTry 'bmk --help'.",
            flag.to_string_lossy()
        )));
    }
    lookup::run(args)
}

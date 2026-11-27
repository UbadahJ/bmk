//! The `bmk` binary. All logic lives in the library crate.

fn main() {
    let args: Vec<std::ffi::OsString> = std::env::args_os().skip(1).collect();
    std::process::exit(bmk::main(&args));
}

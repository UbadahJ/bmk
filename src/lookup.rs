//! Lookup mode: `bmk <segment>...` prints the bookmarked path.

use std::ffi::OsString;
use std::os::unix::ffi::OsStrExt;

use crate::{key, store, write_stdout, Error};

pub fn run(args: &[OsString]) -> Result<(), Error> {
    let key = key::key_from_args(args)?;
    let home = key::home()?;
    let buf = store::read(&store::config_path(&home))?;
    match store::lookup(&buf, key.as_bytes()) {
        store::Found::Path(path) => {
            let mut out = key::expand(path, home.as_os_str().as_bytes()).into_owned();
            out.push(b'\n');
            write_stdout(&out)
        }
        store::Found::Namespace(children) => Err(Error::IsNamespace(
            key,
            children
                .iter()
                .map(|c| String::from_utf8_lossy(c).into_owned())
                .collect(),
        )),
        store::Found::Missing => Err(Error::NotFound(key)),
    }
}

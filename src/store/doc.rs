//! The file as raw lines, edited in place so comments and order survive byte for byte.

use std::borrow::Cow;

use crate::Error;

/// The file as raw lines, edited in place so comments and order survive byte for byte.
pub struct Doc<'a> {
    lines: Vec<Cow<'a, [u8]>>,
    trailing_newline: bool,
}

impl<'a> Doc<'a> {
    pub fn new(buf: &'a [u8]) -> Self {
        let trailing_newline = buf.ends_with(b"\n");
        let body = if trailing_newline {
            &buf[..buf.len() - 1]
        } else {
            buf
        };
        let lines = if buf.is_empty() {
            Vec::new()
        } else {
            body.split(|&b| b == b'\n').map(Cow::Borrowed).collect()
        };
        Doc {
            lines,
            trailing_newline,
        }
    }

    /// Replaces line `no` (1-based, as in `Line::no`).
    pub fn replace(&mut self, no: usize, line: Vec<u8>) -> Result<(), Error> {
        let slot = no
            .checked_sub(1)
            .and_then(|i| self.lines.get_mut(i))
            .ok_or_else(|| out_of_range(no))?;
        *slot = Cow::Owned(line);
        Ok(())
    }

    /// Removes line `no` (1-based, as in `Line::no`).
    pub fn remove(&mut self, no: usize) -> Result<(), Error> {
        let i = no
            .checked_sub(1)
            .filter(|&i| i < self.lines.len())
            .ok_or_else(|| out_of_range(no))?;
        self.lines.remove(i);
        Ok(())
    }

    pub fn push(&mut self, line: Vec<u8>) {
        self.lines.push(Cow::Owned(line));
        self.trailing_newline = true;
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = self.lines.join(&b'\n');
        if self.trailing_newline && !self.lines.is_empty() {
            out.push(b'\n');
        }
        out
    }
}

/// Line numbers come from parsing the same buffer, so this means a bug, not bad input.
fn out_of_range(no: usize) -> Error {
    Error::Config(format!("internal error: line {no} is out of range"))
}

/// Formats a bookmark line: `key<TAB>path[<TAB>tags]`.
pub fn format_line(key: &[u8], path: &[u8], tags: &[u8]) -> Vec<u8> {
    let mut line = Vec::with_capacity(key.len() + path.len() + tags.len() + 2);
    line.extend_from_slice(key);
    line.push(b'\t');
    line.extend_from_slice(path);
    if !tags.is_empty() {
        line.push(b'\t');
        line.extend_from_slice(tags);
    }
    line
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn doc_round_trip() {
        for buf in [&b""[..], b"\n", b"a\n", b"a", b"# c\r\n\na\tb\n"] {
            assert_eq!(Doc::new(buf).to_bytes(), buf);
        }
        let mut d = Doc::new(b"a");
        d.push(b"b".to_vec());
        assert_eq!(d.to_bytes(), b"a\nb\n");
        let mut d = Doc::new(b"");
        d.push(b"b".to_vec());
        assert_eq!(d.to_bytes(), b"b\n");
        let mut d = Doc::new(b"a\nb\nc\n");
        d.remove(2).unwrap();
        d.replace(1, b"A".to_vec()).unwrap();
        assert_eq!(d.to_bytes(), b"A\nc\n");
        assert!(d.replace(0, Vec::new()).is_err());
        assert!(d.replace(3, Vec::new()).is_err());
        assert!(d.remove(3).is_err());
        assert_eq!(d.to_bytes(), b"A\nc\n");
    }
}

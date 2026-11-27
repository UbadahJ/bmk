//! Every failure, with its exit code and stderr message.

use std::io;

#[derive(Debug)]
pub enum Error {
    NotFound(String),
    IsNamespace(String, Vec<String>),
    Conflict(String),
    NoMatches(String),
    /// Findings were already printed; holds their count.
    DoctorFailed(usize),
    /// A short message; the caller includes the "Try ..." hint.
    Usage(String),
    /// Full usage text for stderr (bare `bmk`, bare `bmk --cli`).
    UsageText(&'static str),
    Invalid(String),
    Config(String),
    /// Pre-formatted findings that stopped a write.
    Blocked(String),
    /// Pass through a child's exit status without a message.
    Status(i32),
    Io(io::Error),
}

impl Error {
    pub fn exit_code(&self) -> i32 {
        match self {
            Error::NotFound(_)
            | Error::IsNamespace(_, _)
            | Error::Conflict(_)
            | Error::NoMatches(_)
            | Error::DoctorFailed(_) => 1,
            Error::Usage(_) | Error::UsageText(_) | Error::Invalid(_) => 2,
            Error::Config(_) | Error::Blocked(_) | Error::Io(_) => 3,
            Error::Status(code) => *code,
        }
    }

    pub fn message(&self) -> String {
        match self {
            Error::NotFound(key) => format!("bmk: no bookmark '{key}'\n"),
            Error::IsNamespace(key, children) => {
                format!("bmk: '{key}' is a namespace: {}\n", children.join(", "))
            }
            Error::DoctorFailed(n) => {
                format!("bmk: {n} problem{} found\n", if *n == 1 { "" } else { "s" })
            }
            Error::UsageText(text) => (*text).to_string(),
            Error::Blocked(text) => text.clone(),
            Error::Status(_) => String::new(),
            Error::Conflict(msg)
            | Error::NoMatches(msg)
            | Error::Usage(msg)
            | Error::Invalid(msg)
            | Error::Config(msg) => format!("bmk: {msg}\n"),
            Error::Io(e) => format!("bmk: write error: {e}\n"),
        }
    }
}

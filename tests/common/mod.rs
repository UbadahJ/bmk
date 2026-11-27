//! Shared harness for the end-to-end tests: each test gets its own temporary `$HOME`
//! and runs the real `bmk` binary.
#![allow(dead_code)] // each test file uses a different subset

use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

static COUNTER: AtomicUsize = AtomicUsize::new(0);

pub struct Env {
    pub home: PathBuf,
}

impl Env {
    pub fn new() -> Self {
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        let home = std::env::temp_dir().join(format!("bmk-test-{}-{n}", std::process::id()));
        let _ = fs::remove_dir_all(&home);
        fs::create_dir_all(&home).unwrap();
        Env { home }
    }

    pub fn with(contents: &str) -> Self {
        let env = Env::new();
        env.write(contents.as_bytes());
        env
    }

    pub fn file(&self) -> PathBuf {
        self.home.join(".config/bmk/bookmarks")
    }

    pub fn write(&self, contents: &[u8]) {
        let file = self.file();
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(file, contents).unwrap();
    }

    pub fn home_str(&self) -> &str {
        self.home.to_str().unwrap()
    }

    pub fn cmd(&self, args: &[&str]) -> Command {
        let mut c = Command::new(env!("CARGO_BIN_EXE_bmk"));
        c.args(args)
            .current_dir(&self.home)
            .env("HOME", &self.home)
            .env("PWD", &self.home)
            .env_remove("XDG_CONFIG_HOME")
            .env_remove("BMK_FILE")
            .env_remove("VISUAL")
            .env_remove("EDITOR");
        c
    }

    pub fn contents(&self) -> String {
        fs::read_to_string(self.file()).unwrap()
    }

    pub fn dir(&self, rel: &str) -> PathBuf {
        let d = self.home.join(rel);
        fs::create_dir_all(&d).unwrap();
        d
    }

    pub fn run(&self, args: &[&str]) -> Out {
        Out(self.cmd(args).output().unwrap())
    }
}

impl Drop for Env {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.home);
    }
}

pub struct Out(pub Output);

impl Out {
    pub fn code(&self) -> i32 {
        self.0.status.code().expect("terminated by signal")
    }
    pub fn stdout(&self) -> String {
        String::from_utf8(self.0.stdout.clone()).unwrap()
    }
    pub fn stderr(&self) -> String {
        String::from_utf8(self.0.stderr.clone()).unwrap()
    }
    /// Asserts a failure: the exit code, empty stdout, and a stderr fragment.
    pub fn fails(&self, code: i32, stderr_has: &str) {
        assert_eq!(self.code(), code, "stderr: {}", self.stderr());
        assert_eq!(self.stdout(), "", "stdout must be empty on failure");
        assert!(
            self.stderr().contains(stderr_has),
            "stderr was: {}",
            self.stderr()
        );
    }
    /// Asserts success with nothing on stdout (management writes report on stderr).
    pub fn quiet_ok(&self) -> &Self {
        assert_eq!(self.code(), 0, "stderr: {}", self.stderr());
        assert_eq!(self.stdout(), "");
        self
    }
    pub fn prints(&self, stdout: &str) {
        assert_eq!(self.code(), 0, "stderr: {}", self.stderr());
        assert_eq!(self.stdout(), stdout);
        assert_eq!(self.stderr(), "");
    }
}

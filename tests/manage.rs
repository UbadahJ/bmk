//! Management mode: grammar, add, rm, mv, tag, path, and atomic writes.

mod common;

use std::fs;
use std::path::PathBuf;

use common::{Env, Out};

#[test]
fn cli_without_command_prints_usage_to_stderr() {
    let env = Env::new();
    env.run(&["--cli"]).fails(2, "Usage: bmk --cli <command>");
    env.run(&["-c"]).fails(2, "Commands:");
}

#[test]
fn cli_help_and_version() {
    let env = Env::new();
    let help = env.run(&["-c", "help"]);
    assert_eq!(help.code(), 0);
    assert!(help.stdout().contains("Commands:"));
    assert!(env
        .run(&["-c", "version"])
        .stdout()
        .starts_with("bmk 0.1.0"));
}

#[test]
fn per_command_help() {
    let env = Env::new();
    for (cmd, want) in [
        ("add", "Usage: bmk --cli add"),
        ("rm", "Usage: bmk --cli rm"),
        ("mv", "Usage: bmk --cli mv"),
        ("tag", "Usage: bmk --cli tag"),
        ("ls", "Usage: bmk --cli ls"),
        ("tags", "Usage: bmk --cli tags"),
        ("edit", "Usage: bmk --cli edit"),
        ("doctor", "Usage: bmk --cli doctor"),
        ("path", "Usage: bmk --cli path"),
    ] {
        for flag in ["-h", "--help"] {
            let out = env.run(&["-c", cmd, flag]);
            assert_eq!(out.code(), 0, "{cmd} {flag}");
            assert!(out.stdout().starts_with(want), "{cmd}: {}", out.stdout());
        }
    }
    // Help wins even after other arguments.
    assert_eq!(env.run(&["-c", "add", "a/b", "-h"]).code(), 0);
}

#[test]
fn usage_errors_exit_2() {
    let env = Env::new();
    env.run(&["-c", "nope"]).fails(2, "unknown command 'nope'");
    env.run(&["-c", "add", "a/b"]).fails(2, "missing argument");
    env.run(&["-c", "rm", "a", "b"])
        .fails(2, "unexpected argument 'b'");
    env.run(&["-c", "rm", "-x", "a"])
        .fails(2, "unknown option '-x' for 'rm'");
    env.run(&["-c", "rm", "-f", "a"])
        .fails(2, "unknown option '-f'");
    env.run(&["-c", "add", "a", "/p", "-t"])
        .fails(2, "needs a value");
    env.run(&["-c", "add", "a", "/p", "-t", ""])
        .fails(2, "empty tag list");
    env.run(&["-c", "add", "a", "/p", "-t", "a,,b"])
        .fails(2, "empty tag");
    env.run(&["-c", "add", "a", "/p", "-t", "a b"])
        .fails(2, "invalid tag");
    env.run(&["-c", "add", "a", "/p", "-t", "a", "-t", "b"])
        .fails(2, "more than once");
    env.run(&["-c", "add", "a", "/p", "--tags=a"])
        .fails(2, "unknown option");
    env.run(&["-c", "add", "bad!", "/p"])
        .fails(2, "invalid key 'bad!'");
    env.run(&["-c", "add", "a/", "/p"]).fails(2, "invalid key");
    env.run(&["-c", "add", "a", "/p\tq"])
        .fails(2, "control character");
    env.run(&["-c", "add", "a", ""]).fails(2, "empty path");
    env.run(&["-c", "tag", "a"])
        .fails(2, "either -t <tags> or --clear");
    env.run(&["-c", "tag", "a", "-t", "x", "--clear"])
        .fails(2, "either");
    assert!(
        !env.file().exists(),
        "usage errors must not create the file"
    );
}

#[test]
fn flags_and_positionals_mix_and_double_dash() {
    let env = Env::new();
    env.run(&["-c", "add", "-t", "x", "one", "-f", "/p"])
        .quiet_ok();
    env.run(&["-c", "add", "two", "--", "-dir"]).quiet_ok();
    assert_eq!(env.contents(), "one\t/p\tx\ntwo\t~/-dir\n");
}

#[test]
fn add_creates_file_and_directory() {
    let env = Env::new();
    let out = env.run(&[
        "-c",
        "add",
        "personal/app",
        "~/Dev/app",
        "-t",
        "Kotlin,android,kotlin",
    ]);
    out.quiet_ok();
    assert!(out
        .stderr()
        .contains("added personal/app -> ~/Dev/app [kotlin,android]"));
    assert_eq!(env.contents(), "personal/app\t~/Dev/app\tkotlin,android\n");
    env.run(&["personal", "app"])
        .prints(&format!("{}/Dev/app\n", env.home_str()));
}

#[test]
fn add_normalizes_paths() {
    let env = Env::new();
    let sub = env.dir("a/b");
    let home = env.home_str().to_owned();
    env.run(&["-c", "add", "abs", &format!("{home}/x//y/./")])
        .quiet_ok();
    env.run(&["-c", "add", "home", &home]).quiet_ok();
    env.run(&["-c", "add", "lookalike", &format!("{home}2/x")])
        .quiet_ok();
    env.run(&["-c", "add", "outside", "/opt/../srv/"])
        .quiet_ok();
    let rel = Out(env
        .cmd(&["-c", "add", "rel", "../c"])
        .env("PWD", &sub)
        .current_dir(&sub)
        .output()
        .unwrap());
    rel.quiet_ok();
    let dot = Out(env
        .cmd(&["-c", "add", "dot", "."])
        .env("PWD", &sub)
        .current_dir(&sub)
        .output()
        .unwrap());
    dot.quiet_ok();
    assert_eq!(
        env.contents(),
        format!(
            "abs\t~/x/y\nhome\t~\nlookalike\t{home}2/x\noutside\t/srv\nrel\t~/a/c\ndot\t~/a/b\n"
        )
    );
}

#[test]
fn add_dot_without_pwd_uses_physical_cwd() {
    let env = Env::new();
    let sub = env.dir("sub");
    let stale = env.home_str().to_owned(); // exists, but isn't the current directory
    for pwd in [None, Some("relative/pwd"), Some(stale.as_str())] {
        let mut c = env.cmd(&["-c", "add", "-f", "here", "."]);
        c.current_dir(&sub);
        match pwd {
            Some(p) => c.env("PWD", p),
            None => c.env_remove("PWD"),
        };
        Out(c.output().unwrap()).quiet_ok();
        let out = env.run(&["here"]);
        assert_eq!(out.code(), 0);
        let got = PathBuf::from(out.stdout().trim_end());
        assert_eq!(
            fs::canonicalize(got).unwrap(),
            fs::canonicalize(&sub).unwrap()
        );
    }
}

#[test]
fn add_rejects_control_characters_from_the_current_directory() {
    let env = Env::new();
    let sub = env.dir("tab\there");
    let out = Out(env
        .cmd(&["-c", "add", "t", "."])
        .env("PWD", &sub)
        .current_dir(&sub)
        .output()
        .unwrap());
    out.fails(2, "control character");
    assert!(!env.file().exists(), "nothing written");
}

#[test]
fn add_duplicate_and_force() {
    let env = Env::with("# keep me\nfirst\t/1\nx\t/old\told\nlast\t/3\n");
    env.run(&["-c", "add", "x", "/new"])
        .fails(1, "already exists (use -f");
    env.run(&["-c", "add", "x", "/new", "-t", "fresh", "-f"])
        .quiet_ok();
    assert_eq!(
        env.contents(),
        "# keep me\nfirst\t/1\nx\t/new\tfresh\nlast\t/3\n"
    );
    env.run(&["-c", "add", "--force", "x", "/newer"]).quiet_ok();
    assert_eq!(
        env.contents(),
        "# keep me\nfirst\t/1\nx\t/newer\nlast\t/3\n"
    );
}

#[test]
fn add_conflicts_exit_1_even_with_force() {
    let env = Env::with("personal/app\t/a\ndotfiles\t/d\n");
    let before = env.contents();
    for force in [false, true] {
        let with = |args: &[&'static str]| {
            let mut v = vec!["-c", "add"];
            v.extend_from_slice(args);
            if force {
                v.push("-f");
            }
            v
        };
        env.run(&with(&["personal", "/p"]))
            .fails(1, "'personal' is a namespace");
        env.run(&with(&["dotfiles/x", "/p"]))
            .fails(1, "'dotfiles' is a bookmark, so it can't be a namespace");
        env.run(&with(&["a/b/c", "/p"]))
            .fails(1, "too deep (at most 2 segments)");
    }
    assert_eq!(env.contents(), before);
}

#[test]
fn add_without_trailing_newline() {
    let env = Env::with("a\t/x");
    env.run(&["-c", "add", "b", "/y"]).quiet_ok();
    assert_eq!(env.contents(), "a\t/x\nb\t/y\n");
}

#[test]
fn writes_preserve_comments_order_and_crlf() {
    let original = "\u{feff}# header\r\n\r\nzeta\t/z\r\n  \n# mid\nalpha\t/a\n";
    let env = Env::with(original);
    env.run(&["-c", "add", "new", "/n"]).quiet_ok();
    assert_eq!(env.contents(), format!("{original}new\t/n\n"));
    env.run(&["-c", "rm", "new"]).quiet_ok();
    assert_eq!(env.contents(), original);
    env.run(&["zeta"]).prints("/z\n");
}

#[test]
fn writes_preserve_permissions_and_leave_no_temp_files() {
    use std::os::unix::fs::PermissionsExt;
    let env = Env::with("a\t/x\n");
    fs::set_permissions(env.file(), fs::Permissions::from_mode(0o600)).unwrap();
    env.run(&["-c", "add", "b", "/y"]).quiet_ok();
    env.run(&["-c", "mv", "b", "c"]).quiet_ok();
    let mode = fs::metadata(env.file()).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600);
    let names: Vec<String> = fs::read_dir(env.file().parent().unwrap())
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect();
    assert_eq!(names, ["bookmarks"]);
}

#[test]
fn malformed_file_blocks_writes() {
    for bad in [
        "spaces  /x\n",
        "bad!\t/x\n",
        "k\t\n",
        "k\t/p\tt\textra\n",
        "d\t/1\nd\t/2\n",
        "a\t/1\na/b\t/2\n",
        "k\t/p\ta,,b\n",
        "k\t/p\ta b\n",
    ] {
        let env = Env::with(bad);
        for args in [
            &["-c", "add", "new", "/n"][..],
            &["-c", "rm", "k"],
            &["-c", "mv", "k", "z"],
            &["-c", "tag", "k", "--clear"],
        ] {
            let out = env.run(args);
            out.fails(3, "refusing to write");
            assert!(
                out.stderr().contains("bookmarks:"),
                "{bad:?}: {}",
                out.stderr()
            );
        }
        assert_eq!(env.contents(), bad, "file must be untouched");
    }
}

#[test]
fn non_blocking_findings_allow_writes() {
    let env = Env::with("k\t/p\tDocker,docker\nd/e/f\t/deep\n");
    env.run(&["-c", "add", "b", "/y"]).quiet_ok();
    env.run(&["-c", "ls", "-t", "docker"])
        .prints("k\t/p\tDocker,docker\n");
}

#[test]
fn rm_rules() {
    let env = Env::with("personal/a\t/a\npersonal/b\t/b\nsolo\t/s\n");
    env.run(&["-c", "rm", "nope"])
        .fails(1, "no bookmark 'nope'");
    env.run(&["-c", "rm", "personal"])
        .fails(1, "'personal' is a namespace: a, b");
    env.run(&["-c", "rm", "personal/a"]).quiet_ok();
    assert_eq!(env.contents(), "personal/b\t/b\nsolo\t/s\n");
    Env::new()
        .run(&["-c", "rm", "x"])
        .fails(1, "no bookmark 'x'");
}

#[test]
fn mv_rules() {
    let env = Env::with("a\t/a\tt1\nb/x\t/b\nc\t/c\n");
    env.run(&["-c", "mv", "nope", "z"])
        .fails(1, "no bookmark 'nope'");
    env.run(&["-c", "mv", "b", "z"])
        .fails(1, "'b' is a namespace");
    env.run(&["-c", "mv", "a", "c"])
        .fails(1, "bookmark 'c' already exists");
    env.run(&["-c", "mv", "a", "a"])
        .fails(1, "already called that");
    env.run(&["-c", "mv", "a", "b"])
        .fails(1, "'b' is a namespace");
    env.run(&["-c", "mv", "a", "c/x"])
        .fails(1, "'c' is a bookmark");
    env.run(&["-c", "mv", "a", "q/r/s"]).fails(1, "too deep");
    env.run(&["-c", "mv", "a", "bad!"]).fails(2, "invalid key");
    env.run(&["-c", "mv", "a", "z/a"]).quiet_ok();
    assert_eq!(env.contents(), "z/a\t/a\tt1\nb/x\t/b\nc\t/c\n");
    // A bookmark may move below its own old name.
    env.run(&["-c", "mv", "c", "c/inner"]).quiet_ok();
    assert_eq!(env.contents(), "z/a\t/a\tt1\nb/x\t/b\nc/inner\t/c\n");
}

#[test]
fn tag_rules() {
    let env = Env::with("a\t/a\told\nns/b\t/b\n");
    env.run(&["-c", "tag", "a", "-t", "New,x"]).quiet_ok();
    assert_eq!(env.contents(), "a\t/a\tnew,x\nns/b\t/b\n");
    env.run(&["-c", "tag", "a", "--clear"]).quiet_ok();
    assert_eq!(env.contents(), "a\t/a\nns/b\t/b\n");
    env.run(&["-c", "tag", "ns", "-t", "x"])
        .fails(1, "is a namespace");
    env.run(&["-c", "tag", "zz", "-t", "x"])
        .fails(1, "no bookmark 'zz'");
}

#[test]
fn path_command() {
    let env = Env::new();
    let out = env.run(&["-c", "path"]);
    assert_eq!(out.code(), 0);
    assert_eq!(out.stdout(), format!("{}\n", env.file().display()));
    assert!(out.stderr().contains("doesn't exist yet"));
    env.write(b"");
    env.run(&["-c", "path"])
        .prints(&format!("{}\n", env.file().display()));
}

#[test]
fn help_for_a_command_and_strict_help_version_args() {
    let env = Env::new();
    let add = env.run(&["-c", "help", "add"]);
    assert_eq!(add.code(), 0);
    assert!(
        add.stdout().starts_with("Usage: bmk --cli add"),
        "{}",
        add.stdout()
    );
    assert_eq!(add.stdout(), env.run(&["-c", "add", "-h"]).stdout());
    assert!(env
        .run(&["-c", "help", "help"])
        .stdout()
        .contains("Commands:"));
    env.run(&["-c", "help", "nope"])
        .fails(2, "unknown command 'nope'");
    env.run(&["-c", "help", "add", "rm"])
        .fails(2, "unexpected argument 'rm'");
    env.run(&["-c", "version", "--bogus"])
        .fails(2, "unexpected argument '--bogus'");
    env.run(&["-c", "-V", "x"])
        .fails(2, "unexpected argument 'x'");
}

#[test]
fn tag_value_that_looks_like_a_flag_is_a_missing_value() {
    let env = Env::new();
    for flag in ["-f", "-h", "--force", "--"] {
        env.run(&["-c", "add", "k", "/p", "-t", flag])
            .fails(2, "option '-t' needs a value");
    }
    env.run(&["-c", "add", "k", "/p", "-t", "a,-x"])
        .fails(2, "invalid tag '-x'");
    env.run(&["-c", "add", "k", "/p", "-t", "-"])
        .fails(2, "invalid tag '-'");
    assert!(!env.file().exists());
}

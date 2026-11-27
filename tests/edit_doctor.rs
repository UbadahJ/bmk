//! `edit` and `doctor`.

mod common;

use std::fs;

use common::{Env, Out};

#[test]
fn edit_creates_file_and_runs_visual_with_args() {
    let env = Env::new();
    let out = Out(env
        .cmd(&["-c", "edit"])
        .env("VISUAL", "printf 'k\\t/p\\n' >>")
        .output()
        .unwrap());
    out.quiet_ok();
    assert_eq!(
        env.contents(),
        "# key<TAB>path<TAB>tags (optional)\nk\t/p\n"
    );
    env.run(&["k"]).prints("/p\n");
}

#[test]
fn edit_prefers_visual_then_editor() {
    let env = Env::with("");
    let run = |visual: &str, editor: &str| {
        Out(env
            .cmd(&["-c", "edit"])
            .env("VISUAL", visual)
            .env("EDITOR", editor)
            .output()
            .unwrap())
    };
    run("printf 'v\\t/v\\n' >>", "printf 'e\\t/e\\n' >>").quiet_ok();
    run("  ", "printf 'e\\t/e\\n' >>").quiet_ok();
    // Surrounding whitespace, such as a trailing newline, is trimmed.
    run("printf 'n\\t/n\\n' >>\n", "").quiet_ok();
    assert_eq!(env.contents(), "v\t/v\ne\t/e\nn\t/n\n");
}

#[test]
fn edit_passes_exit_status_and_reports_file_problems() {
    let env = Env::with("ok\t/missing/target\n");
    let failed = Out(env
        .cmd(&["-c", "edit"])
        .env("VISUAL", "exit 7;")
        .output()
        .unwrap());
    assert_eq!(failed.code(), 7);
    assert_eq!(failed.stderr(), "", "targets are never checked by edit");

    let bad = Out(env
        .cmd(&["-c", "edit"])
        .env("VISUAL", "printf 'broken line\\n' >>")
        .output()
        .unwrap());
    assert_eq!(bad.code(), 0);
    assert!(
        bad.stderr().contains("bookmarks:2: missing tab"),
        "{}",
        bad.stderr()
    );
}

#[test]
fn doctor_clean_and_missing_file() {
    let env = Env::new();
    let out = env.run(&["-c", "doctor"]);
    out.quiet_ok();
    env.dir("Dev/app");
    env.write(b"app\t~/Dev/app\tkotlin\n");
    let ok = env.run(&["-c", "doctor"]);
    ok.quiet_ok();
    assert!(ok.stderr().contains("no problems found (1 bookmark)"));
}

#[test]
fn doctor_reports_everything() {
    use std::os::unix::fs::symlink;
    let env = Env::new();
    env.dir("real");
    fs::write(env.home.join("file.txt"), "").unwrap();
    symlink(env.home.join("real"), env.home.join("good-link")).unwrap();
    symlink(env.home.join("gone"), env.home.join("bad-link")).unwrap();
    env.write(
        b"real\t~/real\n\
missing\t~/nope\n\
file\t~/file.txt\n\
good\t~/good-link\n\
broken\t~/bad-link\n\
no tab here\n\
dup\t~/real\n\
dup\t~/real\n\
real/child\t~/real\n\
a/b/c\t~/real\n\
tags\t~/real\tDocker,docker\n",
    );
    let out = env.run(&["-c", "doctor"]);
    assert_eq!(out.code(), 1);
    assert_eq!(out.stdout(), "");
    let err = out.stderr();
    for want in [
        ":2: target does not exist: ~/nope",
        ":3: target is not a directory: ~/file.txt",
        ":5: target does not exist: ~/bad-link",
        ":6: missing tab",
        ":8: duplicate key 'dup' (first on line 7)",
        ":9: 'real/child' is under bookmark 'real' (line 1)",
        ":10: key 'a/b/c' has more than 2 segments",
        ":11: uppercase tag 'Docker'",
        ":11: duplicate tag 'docker'",
        "problems found",
    ] {
        assert!(err.contains(want), "missing {want:?} in:\n{err}");
    }
    assert!(!err.contains(":1:") && !err.contains(":4:"), "{err}");
}

#[test]
fn doctor_flags_relative_paths_without_checking_them() {
    let env = Env::new();
    env.dir("relative/dir"); // exists relative to the cwd, which must not matter
    env.write(b"rel\trelative/dir\nuser\t~other/x\n");
    let out = env.run(&["-c", "doctor"]);
    assert_eq!(out.code(), 1);
    let err = out.stderr();
    assert!(
        err.contains(":1: path is not absolute: 'relative/dir'"),
        "{err}"
    );
    assert!(
        err.contains(":2: path is not absolute: '~other/x'"),
        "{err}"
    );
    assert!(
        !err.contains("target"),
        "relative targets must not be checked: {err}"
    );
    assert!(err.contains("2 problems found"), "{err}");
    // Non-blocking: writes still work.
    env.run(&["-c", "add", "ok", "/abs"]).quiet_ok();
}

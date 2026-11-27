//! Lookup mode: `bmk <segment>...`.

mod common;

use std::fs;
use std::path::Path;

use common::{Env, Out};

const SAMPLE: &str = "\
# key\tpath\ttags
personal/server-containers\t~/Dev/Embedded/Personal/server-containers\tdocker,infra
personal/kotlin-app\t~/Dev/Personal/Kotlin/kotlin-app\tandroid

dotfiles\t~/dotfiles
acme/api\t/opt/work/acme/api
";

#[test]
fn lookup_hit_expands_tilde() {
    let env = Env::with(SAMPLE);
    let want = format!("{}/Dev/Personal/Kotlin/kotlin-app\n", env.home_str());
    env.run(&["personal", "kotlin-app"]).prints(&want);
}

#[test]
fn space_and_slash_forms_are_equivalent() {
    let env = Env::with(SAMPLE);
    let want = format!("{}/Dev/Personal/Kotlin/kotlin-app\n", env.home_str());
    env.run(&["personal/kotlin-app"]).prints(&want);
}

#[test]
fn single_segment_and_absolute_paths() {
    let env = Env::with(SAMPLE);
    env.run(&["dotfiles"])
        .prints(&format!("{}/dotfiles\n", env.home_str()));
    env.run(&["acme", "api"]).prints("/opt/work/acme/api\n");
}

#[test]
fn words_that_look_like_commands_are_bookmarks() {
    let env = Env::with("add\t/a\nls/rm\t/b\n");
    env.run(&["add"]).prints("/a\n");
    env.run(&["ls", "rm"]).prints("/b\n");
}

#[test]
fn miss_exits_1_with_empty_stdout() {
    let env = Env::with(SAMPLE);
    env.run(&["personal", "nope"])
        .fails(1, "no bookmark 'personal/nope'");
}

#[test]
fn missing_file_is_a_miss() {
    let env = Env::new();
    env.run(&["personal", "x"])
        .fails(1, "no bookmark 'personal/x'");
}

#[test]
fn namespace_lists_children() {
    let env = Env::with(SAMPLE);
    env.run(&["personal"]).fails(
        1,
        "'personal' is a namespace: server-containers, kotlin-app",
    );
}

#[test]
fn bookmark_is_not_a_prefix_match() {
    let env = Env::with(SAMPLE);
    env.run(&["personal", "kotlin"])
        .fails(1, "no bookmark 'personal/kotlin'");
    env.run(&["dot"]).fails(1, "no bookmark 'dot'");
}

#[test]
fn bom_and_crlf_are_stripped() {
    let env = Env::new();
    env.write(b"\xEF\xBB\xBFfirst\t/one\r\nsecond\t/two\ttag\r\n");
    env.run(&["first"]).prints("/one\n");
    env.run(&["second"]).prints("/two\n");
}

#[test]
fn malformed_lines_are_skipped() {
    let env = Env::with(
        "spaces/instead  /of/tabs\nbad!\t/x\nempty\t\nfour\t/p\tt\textra\n-dash\t/p\nok\t/fine\n",
    );
    env.run(&["ok"]).prints("/fine\n");
    env.run(&["spaces/instead"]).fails(1, "no bookmark");
    env.run(&["empty"]).fails(1, "no bookmark");
    env.run(&["four"]).fails(1, "no bookmark");
}

#[test]
fn first_match_wins() {
    let env = Env::with("dup\t/first\ndup\t/second\n");
    env.run(&["dup"]).prints("/first\n");
}

#[test]
fn file_without_trailing_newline() {
    let env = Env::with("last\t/p");
    env.run(&["last"]).prints("/p\n");
}

#[test]
fn tilde_rules() {
    let env = Env::with("home\t~\nuser\t~other/x\nmid\t/a/~/b\n");
    env.run(&["home"]).prints(&format!("{}\n", env.home_str()));
    env.run(&["user"]).prints("~other/x\n");
    env.run(&["mid"]).prints("/a/~/b\n");
}

#[test]
fn home_with_trailing_slash() {
    let env = Env::with("x\t~/dir\n");
    let out = env
        .cmd(&["x"])
        .env("HOME", format!("{}/", env.home_str()))
        .output()
        .unwrap();
    assert_eq!(
        String::from_utf8(out.stdout).unwrap(),
        format!("{}/dir\n", env.home_str())
    );
}

#[test]
fn config_location_is_fixed() {
    let env = Env::with("x\t/right\n");
    let xdg = env.home.join("xdg");
    fs::create_dir_all(xdg.join("bmk")).unwrap();
    fs::write(xdg.join("bmk/bookmarks"), "x\t/wrong\n").unwrap();
    let out = env
        .cmd(&["x"])
        .env("XDG_CONFIG_HOME", &xdg)
        .env("BMK_FILE", xdg.join("bmk/bookmarks"))
        .output()
        .unwrap();
    assert_eq!(String::from_utf8(out.stdout).unwrap(), "/right\n");
}

#[test]
fn unset_or_empty_home_exits_3() {
    let env = Env::with(SAMPLE);
    let unset = Out(env.cmd(&["dotfiles"]).env_remove("HOME").output().unwrap());
    unset.fails(3, "HOME is not set");
    let empty = Out(env.cmd(&["dotfiles"]).env("HOME", "").output().unwrap());
    empty.fails(3, "HOME is not set");
}

#[test]
fn unreadable_config_exits_3() {
    let env = Env::new();
    fs::create_dir_all(env.file()).unwrap(); // a directory where the file should be
    env.run(&["x"]).fails(3, "cannot read");
}

#[test]
fn bare_bmk_prints_usage_to_stderr() {
    let env = Env::new();
    env.run(&[]).fails(2, "Usage:");
}

#[test]
fn help_and_version() {
    let env = Env::new();
    let help = env.run(&["--help"]);
    assert_eq!(help.code(), 0);
    assert!(help.stdout().contains("Usage:"));
    assert_eq!(env.run(&["-h"]).stdout(), help.stdout());

    let version = env.run(&["-V"]);
    assert_eq!(version.code(), 0);
    assert!(
        version.stdout().starts_with("bmk 0.1.0 ("),
        "{}",
        version.stdout()
    );
    assert_eq!(env.run(&["--version"]).stdout(), version.stdout());
}

#[test]
fn invalid_segments_exit_2() {
    let env = Env::with(SAMPLE);
    for args in [
        &["bad!"][..],
        &["personal/"],
        &["a//b"],
        &[""],
        &["..", "x"],
        &["a b"],
    ] {
        env.run(args).fails(2, "invalid bookmark name");
    }
}

#[test]
fn options_in_lookup_mode_exit_2() {
    let env = Env::with(SAMPLE);
    env.run(&["-x"]).fails(2, "unexpected option '-x'");
    env.run(&["personal", "-h"])
        .fails(2, "unexpected option '-h'");
    env.run(&["-h", "extra"]).fails(2, "unexpected option");
}

#[test]
fn config_path_helper_matches_plan() {
    let env = Env::new();
    assert!(env.file().ends_with(Path::new(".config/bmk/bookmarks")));
}

#[test]
fn relative_home_exits_3() {
    let env = Env::with(SAMPLE);
    let out = Out(env.cmd(&["dotfiles"]).env("HOME", "rel").output().unwrap());
    out.fails(3, "HOME must be an absolute path (got 'rel')");
    let add = Out(env
        .cmd(&["-c", "add", "r", "~/x"])
        .env("HOME", "rel")
        .output()
        .unwrap());
    add.fails(3, "HOME must be an absolute path");
}

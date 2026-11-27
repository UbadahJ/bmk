//! The `__complete` protocol and fish-driven completion tests.

mod common;

use std::fs;
use std::path::Path;
use std::process::Command;

use common::{Env, Out};

const COMPLETE: &str = "\
personal/server\t~/Dev/one\tdocker,infra
personal/kotlin-app\t~/Dev/two\tandroid
dotfiles\t~/dotfiles
acme/api\t/w\tdocker
bad line
";

fn complete(env: &Env, current: &str, tokens: &[&str]) -> String {
    let mut args = vec!["--cli", "__complete", current];
    args.extend_from_slice(tokens);
    let out = env.run(&args);
    assert_eq!(out.code(), 0);
    assert_eq!(out.stderr(), "", "completion must never write to stderr");
    out.stdout()
}

#[test]
fn complete_lookup_positions() {
    let env = Env::with(COMPLETE);
    assert_eq!(
        complete(&env, "", &[]),
        "personal\tnamespace\ndotfiles\t~/dotfiles\nacme\tnamespace\n"
    );
    assert_eq!(
        complete(&env, "", &["personal"]),
        "server\t~/Dev/one\nkotlin-app\t~/Dev/two\n"
    );
    assert_eq!(
        complete(&env, "", &["dotfiles"]),
        "",
        "nothing below a bookmark"
    );
    assert_eq!(complete(&env, "", &["nope"]), "");
    assert_eq!(complete(&env, "", &["bad!"]), "");
    assert!(complete(&env, "-", &[]).contains("--cli\tManage bookmarks\n"));
    assert_eq!(complete(&env, "", &["personal", "-x"]), "");
    // Slash form offers full relative keys, minus the namespace already typed.
    let slash = complete(&env, "personal/", &[]);
    assert!(slash.contains("personal/server\t~/Dev/one\n"), "{slash}");
    assert!(!slash.contains("personal/\t"), "{slash}");
    // An exact bookmark stays a candidate, so fish doesn't extend it to a longer key.
    let exact = complete(&env, "personal/server", &[]);
    assert!(exact.contains("personal/server\t~/Dev/one\n"), "{exact}");
}

#[test]
fn complete_management_positions() {
    let env = Env::with(COMPLETE);
    let commands = complete(&env, "", &["--cli"]);
    for c in [
        "add\t", "rm\t", "mv\t", "tag\t", "ls\t", "tags\t", "edit\t", "doctor\t", "path\t",
        "fish\t",
    ] {
        assert!(commands.contains(c), "{c} missing");
    }
    assert!(!commands.contains("__complete"));
    assert_eq!(
        complete(&env, "", &["-c", "add"]),
        "personal/\tnamespace\nacme/\tnamespace\n"
    );
    assert_eq!(
        complete(&env, "~/D", &["-c", "add", "x/y"]),
        "__BMK_DIRS__\n"
    );
    assert_eq!(
        complete(&env, "", &["-c", "add", "-t", "a", "x/y"]),
        "__BMK_DIRS__\n"
    );
    assert_eq!(complete(&env, "", &["-c", "add", "x/y", "/p"]), "");
    let keys = "personal/server\t~/Dev/one\npersonal/kotlin-app\t~/Dev/two\ndotfiles\t~/dotfiles\nacme/api\t/w\n";
    assert_eq!(complete(&env, "", &["-c", "rm"]), keys);
    assert_eq!(complete(&env, "", &["-c", "tag"]), keys);
    assert_eq!(complete(&env, "", &["-c", "mv"]), keys);
    assert_eq!(
        complete(&env, "", &["-c", "mv", "dotfiles"]),
        "personal/\tnamespace\nacme/\tnamespace\n"
    );
    assert_eq!(
        complete(&env, "", &["-c", "ls"]),
        format!("personal/\tnamespace\nacme/\tnamespace\n{keys}")
    );
    assert_eq!(complete(&env, "", &["-c", "rm", "dotfiles"]), "");
    assert_eq!(complete(&env, "", &["-c", "tags"]), "");
}

#[test]
fn complete_flags_and_tags() {
    let env = Env::with(COMPLETE);
    let add = complete(&env, "-", &["-c", "add"]);
    assert!(add.contains("--force\t") && add.contains("-t\t") && add.contains("--help\t"));
    let tag = complete(&env, "--", &["-c", "tag", "dotfiles"]);
    assert!(tag.contains("--clear\t") && !tag.contains("--force"));
    assert_eq!(
        complete(&env, "-", &["-c", "rm"]),
        "--help\tShow help\n-h\tShow help\n"
    );
    assert_eq!(
        complete(&env, "-", &["-c", "add", "--", "k"]),
        "__BMK_DIRS__\n"
    );

    let all = "android\t1 bookmark\ndocker\t2 bookmarks\ninfra\t1 bookmark\n";
    assert_eq!(complete(&env, "", &["-c", "ls", "-t"]), all);
    assert_eq!(complete(&env, "do", &["-c", "add", "k", "--tags"]), all);
    assert_eq!(
        complete(&env, "docker,", &["-c", "ls", "-t"]),
        "docker,android\t1 bookmark\ndocker,infra\t1 bookmark\n"
    );
    assert_eq!(
        complete(&env, "Docker,infra,a", &["-c", "tag", "k", "-t"]),
        "Docker,infra,android\t1 bookmark\n"
    );
    // -t only takes a value for commands that accept tags.
    assert_eq!(
        complete(&env, "", &["-c", "rm", "-t"]),
        complete(&env, "", &["-c", "rm"])
    );
}

#[test]
fn complete_never_fails() {
    let env = Env::new();
    assert_eq!(complete(&env, "", &[]), "");
    let no_home = Out(env
        .cmd(&["-c", "__complete", ""])
        .env_remove("HOME")
        .output()
        .unwrap());
    assert_eq!(no_home.code(), 0);
    assert_eq!(no_home.stdout(), "");
    assert_eq!(no_home.stderr(), "");
    let bare = Out(env.cmd(&["-c", "__complete"]).output().unwrap());
    assert_eq!(bare.code(), 0);
}

#[test]
fn fish_command_prints_embedded_script() {
    let env = Env::new();
    let out = env.run(&["-c", "fish"]);
    out.prints(
        &fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/completions/bmk.fish")).unwrap(),
    );
    assert!(out
        .stdout()
        .contains("complete -c bmk -f -a '(__bmk_complete)'"));
}

/// Runs a fish script in an isolated fish (no user config) with our binary on `PATH`.
/// Fish 3.4+ is a hard requirement for the test suite.
fn fish(env: &Env, script: &str, arg: &str) -> String {
    let bin_dir = Path::new(env!("CARGO_BIN_EXE_bmk")).parent().unwrap();
    let path = format!(
        "{}:{}",
        bin_dir.display(),
        std::env::var("PATH").unwrap_or_default()
    );
    let out = Command::new("fish")
        .args(["--no-config", "-c", script, arg])
        .env("HOME", &env.home)
        .env("PATH", path)
        .env_remove("XDG_CONFIG_HOME")
        .current_dir(&env.home)
        .output()
        .expect("fish (>= 3.4) must be installed to run the completion tests");
    assert!(
        out.status.success(),
        "fish failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap()
}

/// Runs `complete -C <line>` with the script loaded via `bmk --cli fish`.
fn fish_complete(env: &Env, line: &str) -> String {
    fish(env, "bmk --cli fish | source; complete -C $argv[1]", line)
}

#[test]
fn fish_completes_bare_and_inside_substitutions() {
    let env = Env::with(COMPLETE);
    let top = fish_complete(&env, "bmk pers");
    assert_eq!(top, "personal\tnamespace\n");
    for line in [
        "bmk personal ",
        "cd \"$(bmk personal ",
        "cd (bmk personal ",
        "code (bmk personal k",
    ] {
        let got = fish_complete(&env, line);
        assert!(got.contains("kotlin-app\t~/Dev/two"), "{line:?}: {got}");
    }
    let flags = fish_complete(&env, "bmk -c add --");
    assert!(
        flags.contains("--force\tReplace an existing bookmark"),
        "{flags}"
    );
}

#[test]
fn fish_completes_quoted_tags() {
    let env = Env::with(COMPLETE);
    let got = fish_complete(&env, "bmk -c ls -t \"docker,");
    assert!(got.contains("docker,android"), "{got}");
    assert!(!got.contains("docker,docker"), "{got}");
}

#[test]
fn fish_completion_ignores_a_bmk_function() {
    let env = Env::with(COMPLETE);
    let got = fish(
        &env,
        "function bmk; echo wrapped; end; command bmk --cli fish | source; complete -C $argv[1]",
        "bmk pers",
    );
    assert_eq!(got, "personal\tnamespace\n");
}

#[test]
fn fish_completes_directories_for_add() {
    let env = Env::with(COMPLETE);
    env.dir("Projects/alpha");
    env.dir("Projects/beta");
    fs::write(env.home.join("Projects/file.txt"), "").unwrap();
    let got = fish_complete(&env, "bmk --cli add personal/x Projects/");
    assert!(got.contains("Projects/alpha/"), "{got}");
    assert!(got.contains("Projects/beta/"), "{got}");
    assert!(!got.contains("file.txt"), "only directories: {got}");
}

#[test]
fn fish_script_prints_nothing_when_there_are_no_candidates() {
    let env = Env::with(COMPLETE);
    // Stand in for the interactive command line, then call the completion function directly.
    let script = r#"
        bmk --cli fish | source
        function commandline
            switch "$argv[1]"
                case -xpc -opc
                    printf '%s\n' bmk -c tags
                case -ct
                    echo ''
            end
        end
        __bmk_complete | count; or true  # count exits 1 when it counts zero
    "#;
    assert_eq!(fish(&env, script, ""), "0\n");
}

#[test]
fn complete_help_offers_commands() {
    let env = Env::with(COMPLETE);
    let got = complete(&env, "", &["-c", "help"]);
    assert!(got.contains("add\t") && got.contains("doctor\t"), "{got}");
    assert!(
        !got.contains("help\t") && !got.contains("version\t"),
        "{got}"
    );
    assert_eq!(complete(&env, "", &["-c", "help", "add"]), "");
}

#[test]
fn complete_ignores_invalid_tags_in_the_file() {
    let env = Env::with("k\t/p\tok,foo bar,,-x\nj\t/q\t   \n");
    assert_eq!(complete(&env, "", &["-c", "ls", "-t"]), "ok\t1 bookmark\n");
}

#[test]
fn ls_namespace_candidate_is_accepted_by_ls() {
    let env = Env::with(COMPLETE);
    // Completion offers `personal/`; fish leaves no trailing space, so that's what gets run.
    assert!(complete(&env, "", &["-c", "ls"]).contains("personal/\tnamespace"));
    let out = env.run(&["-c", "ls", "personal/"]);
    assert_eq!(out.code(), 0, "{}", out.stderr());
    assert_eq!(out.stdout(), env.run(&["-c", "ls", "personal"]).stdout());
}

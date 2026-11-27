//! `ls` and `tags`: filtering, output formats, broken pipes.

mod common;

use common::Env;

const LIST: &str = "\
personal/app\t~/Dev/app\tkotlin,Android
personal/server\t/srv\tdocker,infra
personal2/x\t/p2
acme/api\t~/w/api\tdocker
";

#[test]
fn ls_piped_is_tsv_with_expanded_paths() {
    let env = Env::with(LIST);
    let h = env.home_str().to_owned();
    env.run(&["-c", "ls"]).prints(&format!(
        "personal/app\t{h}/Dev/app\tkotlin,Android\npersonal/server\t/srv\tdocker,infra\npersonal2/x\t/p2\t\nacme/api\t{h}/w/api\tdocker\n"
    ));
}

#[test]
fn ls_prefix_and_tags() {
    let env = Env::with(LIST);
    let h = env.home_str().to_owned();
    env.run(&["-c", "ls", "personal"]).prints(&format!(
        "personal/app\t{h}/Dev/app\tkotlin,Android\npersonal/server\t/srv\tdocker,infra\n"
    ));
    env.run(&["-c", "ls", "personal/server"])
        .prints("personal/server\t/srv\tdocker,infra\n");
    env.run(&["-c", "ls", "-t", "docker"]).prints(&format!(
        "personal/server\t/srv\tdocker,infra\nacme/api\t{h}/w/api\tdocker\n"
    ));
    env.run(&["-c", "ls", "-t", "DOCKER,infra"])
        .prints("personal/server\t/srv\tdocker,infra\n");
    env.run(&["-c", "ls", "-t", "android"])
        .prints(&format!("personal/app\t{h}/Dev/app\tkotlin,Android\n"));
    env.run(&["-c", "ls", "acme", "-t", "docker"])
        .prints(&format!("acme/api\t{h}/w/api\tdocker\n"));
    env.run(&["-c", "ls", "pers"])
        .fails(1, "no bookmarks under 'pers'");
    env.run(&["-c", "ls", "-t", "docker,kotlin"])
        .fails(1, "no bookmarks tagged 'docker,kotlin'");
    env.run(&["-c", "ls", "acme", "-t", "infra"])
        .fails(1, "no bookmarks tagged");
    env.run(&["-c", "ls", "bad!"]).fails(2, "invalid key");
}

#[test]
fn ls_and_tags_on_empty_store() {
    let env = Env::new();
    env.run(&["-c", "ls"]).prints("");
    env.run(&["-c", "tags"]).prints("");
    env.run(&["-c", "ls", "x"])
        .fails(1, "no bookmarks under 'x'");
}

#[test]
fn tags_counts_sorted_case_folded() {
    let env = Env::with(LIST);
    env.run(&["-c", "tags"])
        .prints("android\t1\ndocker\t2\ninfra\t1\nkotlin\t1\n");
}

#[test]
fn ls_on_a_terminal_is_aligned_and_keeps_tilde() {
    use std::process::Command;
    let env = Env::with(LIST);
    // Run under a pseudo-terminal. BSD/macOS and util-linux `script` differ in syntax.
    let mut script = Command::new("script");
    if cfg!(target_os = "macos") {
        script.args(["-q", "/dev/null", env!("CARGO_BIN_EXE_bmk"), "-c", "ls"]);
    } else {
        let cmd = format!("'{}' -c ls", env!("CARGO_BIN_EXE_bmk"));
        script.args(["-qec", &cmd, "/dev/null"]);
    }
    let out = script.env("HOME", &env.home).output().unwrap();
    let text = String::from_utf8(out.stdout).unwrap().replace('\r', "");
    let want = "\
personal/app     ~/Dev/app  kotlin,Android
personal/server  /srv       docker,infra
personal2/x      /p2
acme/api         ~/w/api    docker
";
    assert!(text.ends_with(want), "got:\n{text}");
}

#[test]
fn broken_pipe_is_a_quiet_success() {
    let env = Env::new();
    let mut big = String::new();
    for i in 0..5000 {
        big.push_str(&format!(
            "ns/key-{i}\t/some/fairly/long/path/to/project/number/{i}\ttag\n"
        ));
    }
    env.write(big.as_bytes());
    let mut child = env
        .cmd(&["-c", "ls"])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    drop(child.stdout.take()); // close the read end before bmk writes
    let out = child.wait_with_output().unwrap();
    assert_eq!(
        out.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(out.stderr.is_empty());
}

#[test]
fn ls_accepts_one_trailing_slash_only() {
    let env = Env::with(LIST);
    assert_eq!(
        env.run(&["-c", "ls", "personal/"]).stdout(),
        env.run(&["-c", "ls", "personal"]).stdout()
    );
    env.run(&["-c", "ls", "personal//"]).fails(2, "invalid key");
}

#[test]
fn tags_ignores_invalid_tags_in_the_file() {
    let env = Env::with("k\t/p\tok,foo bar,,-x,OK\nj\t/q\t   \n");
    env.run(&["-c", "tags"]).prints("ok\t1\n");
}

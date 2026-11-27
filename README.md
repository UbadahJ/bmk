# bmk

Scoped directory bookmarks for fish. `bmk` is a pure path resolver: it prints one path and
does nothing else. No shell hook, no `cd` wrapper, no dependencies.

```fish
cd "$(bmk personal server-containers)"
code (bmk acme api)
```

Related projects often live in unrelated trees (`~/Dev/Personal/Kotlin/…`,
`~/Dev/Embedded/Personal/…`, `~/Work/…`). bmk groups them under namespaces of your choosing,
with names you'll remember, without moving anything on disk.

> [!WARNING]
> **This project is entirely AI-generated.** Claude wrote the design, the code, the tests and
> these docs, and Gemini was the secondary reviewer. Gemini made extensive use of the
> [Elephant Goldfish Model](https://github.com/google/elephant-goldfish-model) (EGM), a review pipeline that runs several fresh-context reviewers in
> parallel. EGM ran twice: once over the design and once over the finished code. Before any
> code was written, there was a long planning phase: requirements, a written design
> ([`docs/design.md`](docs/design.md)), and the EGM design review
> ([`docs/design-review.md`](docs/design-review.md)). Several rounds of review suggestions
> followed. **No human has reviewed the code line by line.** It has
> an extensive test suite (unit, end-to-end and fish-driven tests) and has been used and
> benchmarked, but read the code before trusting it with anything important.

## Install

You need fish 3.4+. There are two ways to install, and both use the same `install.sh`.

### Prebuilt binary

Each [release](https://github.com/UbadahJ/bmk/releases) has generic builds for macOS
(`aarch64-apple-darwin`, `x86_64-apple-darwin`) and static Linux builds
(`x86_64-unknown-linux-musl`, `aarch64-unknown-linux-musl`), plus `SHA256SUMS`.

```sh
v=0.1.0 t=x86_64-unknown-linux-musl     # pick your release and target
curl -fLO https://github.com/UbadahJ/bmk/releases/download/v$v/bmk-$v-$t.tar.gz
tar -xzf bmk-$v-$t.tar.gz && ./bmk-$v-$t/install.sh
```

This installs `bmk` to `~/.local/bin` (or `$BMK_BIN_DIR`). On macOS the binaries aren't
notarized: download them with `curl` as above, or see [`docs/macos.md`](docs/macos.md) for
how to allow a browser download.

### From source

You need Rust 1.70+.

```sh
git clone https://github.com/UbadahJ/bmk.git && cd bmk
./install.sh             # native build, fish completions, empty bookmarks file
./install.sh --uninstall # removes bmk and its completions; keeps your bookmarks
```

`install.sh` runs `cargo install` with `-C target-cpu=native`. A native build may not run on
another CPU, so build on each machine rather than copying the binary. bmk's own work is
about 0.1 ms of each call, so a generic build is practically as fast; see
[Performance](#performance).

Either way, the installer writes `~/.config/fish/completions/bmk.fish`, a one-line stub
(`command -q bmk; and command bmk --cli fish | source`) so the completions always match the
installed binary, and creates `~/.config/bmk/bookmarks` if it's missing. `bmk -V` prints
`(native)` or `(generic)`.

## Quick start

Add a few bookmarks. Use `.` for the directory you're in; relative paths work too.

```console
$ bmk -c add personal/kotlin-app ~/Dev/Personal/Kotlin/kotlin-app -t kotlin,android
bmk: added personal/kotlin-app -> ~/Dev/Personal/Kotlin/kotlin-app [kotlin,android]
$ bmk -c add personal/server-containers ~/Dev/Embedded/Personal/server-containers -t docker,infra
bmk: added personal/server-containers -> ~/Dev/Embedded/Personal/server-containers [docker,infra]
$ cd ~/Work/acme/api
$ bmk -c add acme/api . -t go,backend
bmk: added acme/api -> ~/Work/acme/api [go,backend]
$ bmk -c add acme/web ../web -t ts,frontend
bmk: added acme/web -> ~/Work/acme/web [ts,frontend]
$ bmk -c add dotfiles ~/dotfiles
bmk: added dotfiles -> ~/dotfiles
```

Then use them anywhere a path goes:

```fish
cd "$(bmk personal kotlin-app)"      # or: cd "$(bmk personal/kotlin-app)"
code (bmk acme web)
git -C (bmk dotfiles) pull
cp build/app.apk (bmk personal kotlin-app)/dist/
```

Look around:

```console
$ bmk -c ls
personal/kotlin-app         ~/Dev/Personal/Kotlin/kotlin-app           kotlin,android
personal/server-containers  ~/Dev/Embedded/Personal/server-containers  docker,infra
acme/api                    ~/Work/acme/api                            go,backend
acme/web                    ~/Work/acme/web                            ts,frontend
dotfiles                    ~/dotfiles

$ bmk -c ls acme
acme/api  ~/Work/acme/api  go,backend
acme/web  ~/Work/acme/web  ts,frontend

$ bmk -c tags
android   1
backend   1
docker    1
...
```

Mistakes fail loudly, print nothing on stdout, and exit non-zero:

```console
$ bmk personal
bmk: 'personal' is a namespace: kotlin-app, server-containers
$ bmk personal kotlin
bmk: no bookmark 'personal/kotlin'
$ bmk -c add acme/api /somewhere/else
bmk: bookmark 'acme/api' already exists (use -f to replace it)
```

Rename, retag, remove and check:

```console
$ bmk -c mv acme/web acme/frontend
bmk: renamed acme/web -> acme/frontend
$ bmk -c tag dotfiles -t config            # replaces the tags; --clear removes them
bmk: tagged dotfiles -> ~/dotfiles [config]
$ bmk -c rm dotfiles
bmk: removed dotfiles -> ~/dotfiles [config]
$ bmk -c doctor                            # after deleting ~/Work/acme/api
~/.config/bmk/bookmarks:4: target does not exist: ~/Work/acme/api
bmk: 1 problem found
```

`bmk -c help` lists every command, and `bmk -c help <command>` shows one. Without `--cli`
(or `-c`), every word is part of a bookmark name, so a bookmark can even be called `add` or
`ls`.

## Recipes

When piped, `bmk -c ls` prints tab-separated `key`, `path` and `tags`, with `~` expanded, so it
composes with other tools:

```fish
# Pick a bookmark with fzf. If you cancel, cd "" fails and you stay put.
cd "$(bmk -c ls | fzf | cut -f2)"

# Run something in every bookmark with a tag (all given tags must match).
for dir in (bmk -c ls -t kotlin | cut -f2)
    git -C $dir status -sb
end

# Everything under one namespace.
for dir in (bmk -c ls acme | cut -f2)
    du -sh $dir
end

# Open a tmux window in a bookmark.
tmux new-window -c "$(bmk acme api)"
```

Shorter to type, if you like (none of this is required; bmk itself installs no functions):

```fish
abbr -a cdb --set-cursor 'cd "$(bmk %)"'     # fish 3.6+: type cdb, then the bookmark
abbr -a b 'bmk -c'                           # b ls, b add work/x .
```

### Quote `cd`

In fish, a command substitution that prints nothing becomes *zero arguments*. So if a
bookmark doesn't exist, `cd (bmk typo)` turns into a bare `cd` and takes you to `$HOME`.
Quote the substitution instead:

```fish
cd "$(bmk personal app)"     # unknown bookmark: cd fails, and you stay where you are
```

The same goes for any command that does something with no arguments (`find`, `ls`, `rm -r`…).
Quote the substitution, or guard it:

```fish
set -l p (bmk personal app); and find $p -name '*.kt'
```

### Completion

Tab completes namespaces and names (inside `(...)` and `"$(...)"` too), `--cli` commands,
each command's flags, tags (including after a comma: `-t docker,<TAB>`), and directories for
`add`:

```text
bmk <TAB>                    acme  dotfiles  personal
bmk personal <TAB>           kotlin-app  server-containers
cd "$(bmk acme <TAB>         api  web
bmk -c ls -t docker,<TAB>    docker,android  docker,backend  …
```

## The bookmarks file

The file is `~/.config/bmk/bookmarks`. The location is fixed; there are no environment
overrides. It's plain text with one bookmark per line, and the fields are separated by
**tab characters**:

```text
# key<TAB>path<TAB>tags (optional)
personal/server-containers	~/Dev/Embedded/Personal/server-containers	docker,infra
personal/kotlin-app	~/Dev/Personal/Kotlin/kotlin-app	android
dotfiles	~/dotfiles
```

- **Keys** are `<namespace>/<name>` or `<name>` (at most 2 segments for now). Names use
  `A-Z a-z 0-9 . _ -`, can't start with `-`, and can't be only dots. A key is either a
  bookmark or a namespace, never both.
- **Paths** are stored absolute. Your home directory is written as `~`.
- **Tags** use `A-Z a-z 0-9 . _ -`, can't start with `-`, are comma-separated, and are
  lowercased when you add them.
- **Reads** never check whether the target directories exist. Only `doctor` does.
- **Writes** are atomic: bmk writes a temp file next to the original, then renames it over the
  original. Comments, blank lines, line order and file permissions are kept.
- **A damaged file:** reads skip lines bmk can't parse. Writes refuse to change a file that
  has problems; they print each one as `file:line: reason` and suggest `bmk -c doctor`. The
  usual cause is an editor that turned tabs into spaces.

## Exit codes

| Code | Meaning |
|---|---|
| 0 | Success (including a closed pipe, as in `bmk -c ls \| head`) |
| 1 | Not found, a namespace where a bookmark was expected, a conflict, no matches, `doctor` findings |
| 2 | Usage error or invalid input; bare `bmk` |
| 3 | The file can't be read or written, it has problems that block a write, or `$HOME` is unset |

On failure, stdout is always empty.

## Performance

A lookup takes about 2.1 ms on an M1 Max (hyperfine median, 1,001-line file). Almost all of
that is macOS starting the process. Reading and scanning the file takes about 0.1 ms.

| Command | Median |
|---|---:|
| `/usr/bin/true` | 1.13 ms |
| empty Rust binary | 1.90 ms |
| `bmk personal kotlin-app` | 2.15 ms |
| `bmk --cli __complete '' personal` | 2.20 ms |

[`docs/benchmark.md`](docs/benchmark.md) has the full breakdown: comparisons with C, Swift
and Objective-C, why a daemon wouldn't help, in-shell lookups, and Linux estimates. Run
`scripts/bench.sh` to reproduce it.

## Development

```sh
cargo test                                   # unit, integration and fish-driven tests
cargo clippy --all-targets -- -D warnings
cargo fmt --check
cargo build --release && ./target/release/bmk -V
```

There are no dependencies, including for tests. The tests run the real binary with a temporary
`$HOME`. The fish completion tests use `fish --no-config` and need fish 3.4+
on `PATH`.

| Path | Contents |
|---|---|
| `src/main.rs` | The binary; forwards arguments to `bmk::main` |
| `src/lib.rs` | Mode switch (lookup vs `--cli`) and top-level dispatch |
| `src/lookup.rs` | Lookup mode |
| `src/cli/` | `--cli` argument parser (`mod.rs`) and every help text (`help.rs`) |
| `src/commands/` | One file per management command, plus shared helpers in `mod.rs` |
| `src/store/` | File location and lenient scanner (`mod.rs`), strict validation (`check.rs`), line-preserving edits (`doc.rs`), atomic write (`write.rs`) |
| `src/key.rs` | Key, segment and tag rules, `~` handling, path normalization |
| `src/complete.rs` | Hidden `__complete` protocol |
| `src/error.rs`, `src/output.rs` | `Error` and exit codes; the only stdout/stderr writers |
| `completions/bmk.fish` | Fish completion script, embedded in the binary |
| `tests/` | End-to-end tests by area, sharing `tests/common/mod.rs` |
| `docs/design.md` | Design document; `docs/design-review.md` is its review |
| `docs/benchmark.md`, `scripts/bench.sh` | Benchmark results and the script that reproduces them |
| `docs/macos.md` | Allowing an unnotarized release binary on macOS |
| `.github/workflows/release.yml` | Tagged releases: tests, generic builds for four targets, GitHub release |

See `AGENT.md` for working on the code.

## License

MIT. See [`LICENSE`](LICENSE).

# AGENT.md

Guidance for coding agents working on `bmk`. `README.md` covers usage; `docs/design.md` is
the design and the source of truth for behavior. If the code and the design disagree, ask
before changing either.

## What this is

A tiny Rust CLI that resolves scoped bookmarks to paths for fish command substitution:
`bmk personal kotlin-app` prints one path. Management commands sit behind `--cli` / `-c`.
The bookmarks file is `~/.config/bmk/bookmarks`, a tab-separated file with one line per
bookmark: `key<TAB>path[<TAB>tags]`.

## Commands

```sh
cargo test                                   # must pass
cargo clippy --all-targets -- -D warnings    # must be clean
cargo fmt                                    # always run before finishing
cargo build --release && ./target/release/bmk -V   # expect "(native)"
```

Run all three (test, clippy, fmt) before calling any change done. `fish` (3.4+) is required:
the fish-driven completion tests fail if it isn't on `PATH`.

To benchmark, run `scripts/bench.sh` (needs `hyperfine` and `python3`) and update
`docs/benchmark.md` with the results. Compare the gaps between rows (lookup − `bmk -V`,
`bmk -V` − empty Rust), not only the gap to `/usr/bin/true`: on macOS, process start takes
almost all of the time, and bmk's own work is about 0.1 ms.

## Releases

Bump `version` in `Cargo.toml`, commit, then push a matching tag (`git tag v0.2.0 && git push
origin v0.2.0`). `.github/workflows/release.yml` runs the tests on macOS and Linux and a build
on Rust 1.70, then builds generic binaries (empty `RUSTFLAGS` overrides `target-cpu=native`)
for `aarch64`/`x86_64` macOS and static `aarch64`/`x86_64` musl Linux. It publishes them as
a GitHub release with `SHA256SUMS`. The tag must match `Cargo.toml`, or the release job fails.
Running the workflow by hand is a dry run: it builds the archives but publishes nothing.
There's no CI on ordinary pushes, so run the checks above locally.

## Hard constraints (settled decisions; don't reopen them)

- **Std only.** No crates in `[dependencies]` or `[dev-dependencies]`. `rust-version = "1.70"`:
  don't use newer std APIs (e.g. `OsStr::as_encoded_bytes` is 1.74, `trim_ascii` is 1.80,
  `File::lock` is 1.89). Unix only: `std::os::unix` is fine.
- **Pure resolver.** No shell init hook and no `cd` wrapper. Lookup prints exactly
  `<path>\n` to stdout or nothing, and never has side effects.
- **Never touch targets.** Lookup, `ls`, `tags`, completion, writes and `edit` must never
  stat, canonicalize or validate bookmark target paths. Only `doctor` calls `fs::metadata`
  on targets.
- **Fixed location.** `$HOME/.config/bmk/bookmarks`. No `BMK_FILE`, `XDG_CONFIG_HOME` or flag
  overrides. Symlinked config files are not supported.
- **Mode switch.** Without `--cli`/`-c` as the first argument, every argument is a bookmark
  segment. Segments can't start with `-`, so flags never collide with names. Don't add
  subcommands or flags to lookup mode.
- **Management keys are one `/`-joined token** (`add personal/app <path>`), so the grammar
  stays unambiguous when nesting arrives. Lookup accepts both `a b` and `a/b`.
- **`MAX_DEPTH = 2`** (`src/key.rs`), enforced only on `add`/`mv` and reported by
  `doctor`. Lookup, `ls` and completion must keep working at any depth.
- **Release profile** (`lto = "fat"`, `codegen-units = 1`, `panic = "abort"`, `strip`) and
  `target-cpu=native` are deliberate. `install.sh` sets `RUSTFLAGS` explicitly because
  `cargo install` doesn't reliably read `.cargo/config.toml`.

## Behavioral contracts tests rely on

- **Exit codes:** 0 success; 1 not found / namespace / conflict / no matches / doctor
  findings; 2 usage or invalid input; 3 I/O, missing `$HOME`, or a write blocked by file
  problems. Map new errors through `Error::exit_code()` in `src/error.rs`.
- **stdout is empty on every failure.** Diagnostics go to stderr via `note()` or
  `Error::message()`.
- **No panics in runtime paths.** With `panic = "abort"`, a panic is a SIGABRT. `src/lib.rs`
  denies `unwrap_used`, `expect_used`, `panic`, `todo`, `unimplemented`, `print_stdout` and
  `print_stderr` outside tests, so clippy enforces this; don't `allow` them. Write output
  with `write_stdout()` (EPIPE is a quiet success) and `note()`. Indexing isn't linted, so
  prefer `get`/`get_mut` unless an index is guarded right next to it.
- **Reads are lenient, writes are strict.** `store::entries()` skips malformed lines.
  Every write goes through `store::update(file, |lines, doc| ...)`, which reads once,
  validates once, and refuses on *blocking* findings from `store::check()`. Uppercase tags, duplicate tags on a line, over-deep keys and hand-edited relative
  paths are non-blocking. `doctor` reports relative paths but never checks them on disk,
  because the result would depend on the current directory.
- **Writes keep the file intact.** Change lines only inside the `store::update` closure,
  through `Doc` (replace, remove or push one line). `update` then persists the result with
  `write_atomic` (sibling temp file, copy permissions, `sync_all`, `rename`), and writes
  nothing if the closure returns an error. Only `edit` calls `write_atomic` directly, to
  create a missing file. Never rewrite the whole file from parsed entries:
  comments, blank lines, CRLF, BOM and ordering must survive byte for byte.
- **Path normalization on `add`** is lexical only (`key::normalize_path`): it expands
  `~`, joins `$PWD` (or `current_dir()`), cleans `.`/`..`/`//`, then rewrites the
  `$HOME` prefix to `~` only on a path-component boundary.
- **`__complete` never fails.** It always exits 0, never writes stderr, and prints
  `candidate<TAB>description` lines, or the single line `__BMK_DIRS__` when fish should
  complete directories. Its tokens are raw data, never parsed as flags.
- **Completion script.** `completions/bmk.fish` is embedded with `include_str!` and printed
  by `bmk --cli fish`. If you change it, update `complete.rs` and the fish-driven tests
  together.

## Code map

| File | Owns |
|---|---|
| `src/main.rs` | The binary: collects args and calls `bmk::main`. Keep it this thin |
| `src/lib.rs` | Mode switch: lookup vs `--cli`, `-h`/`-V`, unexpected options |
| `src/error.rs` | `Error`, `exit_code()`, `message()` |
| `src/output.rs` | `write_stdout` (EPIPE is a quiet success) and `note` (stderr) |
| `src/lookup.rs` | Lookup mode |
| `src/key.rs` | Segment/key/tag validation, `MAX_DEPTH`, `~` expand/contract, path cleaning |
| `src/cli/mod.rs` | `SPECS`: the one table of commands, flags and summaries used by the parser, `help <command>` and completion. Also the parser and `Cmd` |
| `src/cli/help.rs` | Every usage/help/version text |
| `src/commands/mod.rs` | `run` dispatch and shared helpers (`Ctx`, `find`, `not_a_bookmark`, `check_new_key`) |
| `src/commands/*.rs` | One command each; `ls.rs` also has `tags`. Entry point is `pub(super) fn run` |
| `src/store/mod.rs` | Location, `read`, `entries` (lenient), `lookup`, `update` (the only write path), `tag_counts`, key-prefix helpers |
| `src/store/check.rs` | `check`, `Finding`, `report`; the strict pre-write gate used by `update` |
| `src/store/doc.rs` | `Doc` (line-preserving edits), `format_line` |
| `src/store/write.rs` | `write_atomic` |
| `src/complete.rs` | `__complete` protocol and `FISH_SCRIPT` |
| `build.rs` | Sets `BMK_BUILD` to `native` or `generic` for `bmk -V` |
| `tests/common/mod.rs` | `Env` (temporary `HOME` per test) and `Out` assertions |
| `tests/{lookup,manage,list,edit_doctor,complete}.rs` | End-to-end tests by area |
| `install.sh` | Native build from source, or a prebuilt binary from a release archive; fish completion stub, config init, `--uninstall` |
| `.github/workflows/release.yml` | Release on a `v*` tag: tests (macOS, Linux), MSRV build, generic builds, GitHub release |
| `scripts/bench.sh` | Benchmark suite behind `docs/benchmark.md` |

The library is private apart from `bmk::main`; it exists so the binary stays thin and the
modules are reachable for unit tests. Keep new items `pub(crate)` or narrower.

## Adding or changing things

- **New management command:**
  1. Add an entry to `SPECS` in `cli/mod.rs` (name, summary, flags, positional counts),
     its help text in `cli/help.rs`, a line in `help::CLI`, and a `Cmd` variant. Unit
     tests fail if the `help::CLI` line or the usage text is missing.
  2. Implement it as `src/commands/<name>.rs` with `pub(super) fn run`, and dispatch it
     from `commands/mod.rs`. If it writes, use `store::update`.
  3. Command-name and flag completion come from `SPECS` automatically. Only add positional
     cases to `complete::management` if its arguments need candidates.
  4. Add integration tests for the happy path, every error exit code, and `-h`.
- **New file-level rule:** add it to `check()` in `store/check.rs` and decide whether it's blocking. Add
  a case to `strict_findings` (unit, `store/check.rs`) and to
  `malformed_file_blocks_writes` (`tests/manage.rs`) or `doctor_reports_everything`
  (`tests/edit_doctor.rs`).
- **Raising `MAX_DEPTH`:** change the constant, then update the depth tests. Also check
  that completion still offers the right namespaces at each level, including `ns/sub/`
  prefixes for `add`.
- **Tests:** put them in the `tests/` file for that area, and use `mod common;` with the
  `Env` helper from `tests/common/mod.rs`. It creates a temporary `HOME`, sets
  `PWD`, and clears `XDG_CONFIG_HOME`, `BMK_FILE`, `VISUAL` and `EDITOR`. Assert failures with
  `.fails(code, stderr_fragment)`, which also checks that stdout is empty. Never touch the
  real `~/.config`.

## Things that look like bugs but aren't

- `cd (bmk typo)` going to `$HOME` is fish behavior (an empty substitution becomes zero
  arguments). The documented fix is `cd "$(bmk …)"`. Don't add a wrapper.
- `ls` keeps `~` in a terminal but expands it when piped. That's on purpose: piped output
  has to be usable by other tools.
- `edit` runs `sh -c "$editor \"\$1\"" sh <file>` so editor values with arguments
  (`code -w`) work. Tests use this to script the "editor".
- Lookup is first-match-wins on duplicate keys. Writes refuse while duplicates exist.

# Goldfish Review Report

**Target:** `/private/tmp/space/bmk/claude-plan.md` (lines 1-202)
**Mode:** REVIEW. No code exists yet, so the codebase-details lens checked internal consistency and the accuracy of technical claims.
**Settled decisions respected:** Rust std only and no crates; a pure resolver with no hook or `cd` wrapper; never stat targets; a line-based TSV config; v1 with 2 segments that is nesting-ready; tags; speed over size; `target-cpu=native` built per machine. None of the findings argues against these. Several flag ways the plan as written silently defeats one of them.

## Reviewer Status

| Reviewer | Status |
| --- | --- |
| `structural` | completed |
| `style` | completed |
| `comprehension` | completed |
| `design` | completed |
| `zoom-out` | completed |
| `implementation-readiness` | completed |
| `codebase-details` | completed |

**After deduplication:** 4 blocking, 18 should-fix, 16 nit.

**Overall verdict:** the plan's shape is sound and close to minimal. It is **not ready to implement** for two reasons. Four defects would silently break a settled decision or corrupt data. Several behaviors (grammar, malformed lines, path normalization, completion protocol) are still left for the implementer to decide.

---

## Blocking

### B1. `target-cpu=native` is silently dropped by the documented install
* **Reviewer lens:** codebase-details, zoom-out, design, structural, implementation-readiness, comprehension, style (7/7)
* **Location:** lines 129, 164-168, 172 (Implementation tree, Build)
* **Problem:** `cargo install` skips local config discovery. Discovery starts at `$CARGO_HOME/config.toml`, so `cargo install --git ...` (the primary command on line 172) never reads the repo's `.cargo/config.toml`. The binary builds for the generic baseline and nothing reports it. The one exception is `--path <dir>`, which reads `<dir>/.cargo/config.toml`. Separately, a `RUSTFLAGS` environment variable replaces `build.rustflags` entirely, so a stray `RUSTFLAGS` also drops the native flag.
* **Fix:** make the documented install either `RUSTFLAGS="-C target-cpu=native" cargo install --git <url>` or "clone, then `cargo install --path .`". Keep `.cargo/config.toml` for local `cargo build`/`cargo test`. Add one sentence on `RUSTFLAGS` precedence. Optionally, have `bmk -V` report whether it was built native, for example via a `build.rs` env var or `cfg!(target_feature = "avx2")`, so the decision can be verified.

### B2. The completion design contradicts itself and cannot be built as written
* **Reviewer lens:** structural, design, comprehension, zoom-out, implementation-readiness, codebase-details, style (7/7)
* **Location:** lines 112, 115-122
* **Problem:**
  - `complete -c bmk -f -a '(...)'` is one unconditional rule, and `-f` disables file completion at every position. Line 118 ("file/directory completion takes over for `<path>`") therefore cannot happen.
  - Line 115 says `--complete` "drops flags". It then cannot know it is after `-a` (path mode) or `-t` (tag mode, line 119), and it would treat the value of `-t a,b` as a key segment.
  - Once nesting exists, the token after `-a personal kotlin` could be a deeper segment or the path. Line 69's "stays unambiguous" holds for the finished command line, not while typing.
  - Completion of the flags themselves (`bmk -<TAB>`) is not covered.
* **Fix:** write out the full `completions/bmk.fish` and the `--complete` protocol in the plan.
  - `--complete` receives all tokens, flags included, and runs the same state machine as the parser (see S1). A flag that takes a value consumes the next token.
  - Define the states in a small table: nothing typed gives namespaces; after a namespace, its children; after `-t`, tags; after `-l`/`-r`, segments; after `-a` plus `MAX_DEPTH` segments, the path.
  - Path mode needs either a second rule, e.g. `complete -c bmk -n '<needs-path>' -F` (or `-a '(__fish_complete_directories)'`), or a sentinel/exit code from `--complete` that the script maps to directory completion. For future nesting, offer both children and directories at the ambiguous position.
  - Add `complete -c bmk -s a -l add -d ...` style entries for every flag.
  - Add a fish-driven test, e.g. `fish -c "complete -C 'bmk -a personal x '"`, covering both bare and `"$(...)"` forms.

### B3. Atomic rename replaces a symlinked config file and silently breaks dotfiles sync
* **Reviewer lens:** design, zoom-out, comprehension, structural, implementation-readiness, codebase-details (6/7)
* **Location:** lines 54-59 (Storage)
* **Problem:** line 55 says the file syncs via dotfiles. Dotfile managers (stow, chezmoi symlink mode, home-manager, a plain `ln -s`) usually make `~/.config/bmk/bookmarks` a symlink. Writing a temp file next to it and calling `rename` over the path replaces the link with a regular file. From then on every `--add`/`--rm` writes to an unsynced local copy. The rename also resets the file mode to the temp file's umask default.
* **Fix:** in Storage, specify the write algorithm.
  1. Resolve the config path with `fs::canonicalize`. This touches only the config file, not a target, so it is consistent with "never stat targets".
  2. Create the temp file in the resolved file's directory with `OpenOptions::create_new` and a unique name (`.bookmarks.tmp.<pid>`), and remove it on error.
  3. Copy the original permissions, optionally `sync_all`, then `rename` onto the resolved path.

  Add an integration test with a symlinked `BMK_FILE`.

### B4. `--add` can write lines the format cannot hold, and stores relative paths as typed
* **Reviewer lens:** comprehension, implementation-readiness, design, zoom-out, structural, codebase-details, style
* **Location:** lines 44, 54-56, 69, 80-81
* **Problem:**
  - A path or tag containing `\t`, `\n` or `\r` corrupts the line-based file or shifts fields. `env::args_os` can also deliver non-UTF-8 bytes, while line 44 declares the file UTF-8.
  - Only `.` is normalized. `bmk -a p x src` or `../x` would be stored relative and then resolve against whatever directory the caller is in later. With nesting, a forgotten path (`bmk -a personal kotlin game`) silently becomes key `personal/kotlin` pointing at `./game`.
  - Appending to a file that does not end in `\n` (common after hand edits) glues the new entry onto the last line.
* **Fix:** add one "Path normalization on add" rule.
  - Reject `\t`, `\n`, `\r` and non-UTF-8 in paths and tags with exit 2.
  - Join every non-absolute path onto `$PWD` lexically, with no stat. Use `$PWD` only if it is absolute; otherwise fall back to `env::current_dir`, and note that this returns the physical path, e.g. `/private/tmp` on macOS. Say whether `.`/`..` are resolved textually, and strip a trailing `/`.
  - Then apply the `~` rewrite (S4).
  - If the file lacks a trailing newline, write one before appending.
  - Echo the stored path to stderr on add so mistakes are visible.

---

## Should-fix

### S1. The argument grammar is unspecified, and `-t` has two meanings with two list syntaxes
* **Reviewer lens:** all 7
* **Location:** lines 66-77, 147, 199
* **Problem:** as an `--add` option, `-t a,b` takes one comma-separated value. As a top-level command, `-t <tag>...` takes space-separated positionals. Open question 199 suggests a third form. The plan also does not say:
  - whether flags may follow positionals;
  - whether `-af` bundling or `--tag=a` is accepted;
  - whether `--` ends options, which is needed for a directory named `-dir`;
  - that everything after `--complete` is raw data.

  A hand-rolled parser over `args_os` leaves all of this to the implementer.
* **Fix:** add a short formal grammar.
  - The first token is a command flag, or a segment for lookup.
  - Inside `--add`, `-t <csv>` and `-f` may appear anywhere.
  - `--` ends options.
  - There is no bundling and no `=` form, and unknown flags exit 2.
  - Use one tag-list syntax everywhere: comma-separated, with comma meaning AND, which closes question 199. Consider renaming the query to `--tagged`/`-T` so `-t` only ever means "these tags".

### S2. Parser behavior on malformed lines and the meaning of exit 3 are undefined
* **Reviewer lens:** implementation-readiness, design, codebase-details, structural, comprehension
* **Location:** lines 44-52, 91, 143
* **Problem:** `bmk -e` opens the file in `$EDITOR`. Editors with expandtab turn tabs into spaces, so lines with no tab will exist. The plan does not cover lines with no tab, 4+ fields, an empty key or path, CRLF, a BOM, or invalid UTF-8. Exit 3 says "parse error", but the scanner stops at the first match, so it is unclear whether a bad line elsewhere fails the lookup.
* **Fix:**
  - Lookup and completion are lenient: skip lines without a tab, strip a trailing `\r` and a leading BOM, and treat an empty third field as no tags. Optionally print a one-line stderr warning with the line number. Exit 3 is for I/O errors only.
  - Mutating commands and `--check` are strict: they report `path:line: reason` and exit 3 before writing.
  - Field split: first tab, second tab; a third tab is an error in strict mode.

### S3. Duplicate and conflict precedence, namespace detection and `-f` semantics are undefined
* **Reviewer lens:** structural, design, comprehension, zoom-out, implementation-readiness, codebase-details, style
* **Location:** lines 30-31, 57, 69, 82, 143
* **Problem:**
  - "Stop at the first match" makes duplicates first-wins. It also lets a bookmark `personal` win over namespace `personal/*` in a hand-edited file.
  - Detecting a namespace (line 82: exit 1 plus a children listing) requires a full scan for `key/`, which the hot-path description omits.
  - If `-f` appends a new line, the old line still wins on lookup, so the overwrite has no effect.
  - It is unstated whether `-f` can override a namespace/bookmark conflict, and what a rejected add exits with.
* **Fix:**
  - Lookup is first-match-wins. On a miss, the same pass collects `key/` children to choose between "namespace" and "not found". Conflicts and duplicates are reported only by `--check`.
  - `--add` scans the whole file for conflicts and exits 1 on rejection.
  - `-f` replaces only an existing bookmark with the same key, in place, including its tags, and never overrides a namespace conflict.
  - Add `-f` to the CLI table.

### S4. `~` handling has boundary and environment gaps
* **Reviewer lens:** comprehension, implementation-readiness, design, zoom-out, structural, codebase-details, style
* **Location:** lines 54-55, 80-81, 145
* **Problem:**
  - A naive prefix rewrite with `HOME=/Users/one` turns `/Users/one2/x` into `~2/x`.
  - It is unspecified which forms are expanded on output (`~user`?) and what happens if `$HOME` is unset or empty.
  - fish expands `~/x` before bmk sees it, so explicit absolute paths under `$HOME` also need the rewrite, not only `.`.
* **Fix:** rewrite only when the path equals `$HOME` or starts with `$HOME/`, after stripping a trailing `/` from `$HOME`. Apply it to every added path. Expand only a leading `~` or `~/`; never `~user`. If `$HOME` is unset or empty when needed, exit 3.

### S5. Depth rules are inconsistent ("exactly 2" vs `MAX_DEPTH`)
* **Reviewer lens:** zoom-out, design, structural, implementation-readiness, comprehension, style
* **Location:** lines 16, 25, 32, 34, 74
* **Problem:**
  - Line 16 says "exactly 2 segments" while line 34 defines only a maximum, so it is unclear whether `bmk -a dotfiles ~/dotfiles` is legal.
  - The non-goal "nesting deeper than 2 levels" (line 25) conflicts with "resolution and completion work for any depth".
  - "Enforced on add only" conflicts with `--check` linting depth (line 74).
  - The segment regex accepts `.` and `..`.
* **Fix:**
  - State `1..=MAX_DEPTH`, or exactly `MAX_DEPTH` (then add `MIN_DEPTH`).
  - Reword the non-goal to "adding keys with more than 2 segments".
  - Say `--add` rejects over-depth keys and `--check` reports them, and say whether as an error or a warning.
  - Reject segments made only of dots.
  - Soften line 16: lifting the limit also touches `--check` and completion, not just a constant.

### S6. The minimum fish version and the `commandline` flag are left to the implementer
* **Reviewer lens:** codebase-details, design, comprehension, structural, implementation-readiness, zoom-out, style
* **Location:** lines 102, 112, 120-122
* **Problem:** `commandline -x` / `--tokens-expanded` arrived in fish 4.0, and older fish errors on it, which breaks completion entirely. Line 102 implies fish 3.4 or later, which `"$(...)"` requires. "Check against the installed version" is a to-do, not a design. Two claims are unverified:
  - that completion works inside a quoted `"$(bmk pers|)"`;
  - that `-p` scopes to the innermost substitution, though that one is fairly standard.
* **Fix:** state one minimum fish version and pick the flag.
  - `-opc` works on 3.4 through 4.x, but is deprecated in 4.x.
  - `-xpc` requires documenting fish 4.0 or later.
  - Alternatively, detect at runtime: `commandline -xpc 2>/dev/null; or commandline -opc`.

  Reword line 115: `-c` cuts at the cursor, so `--complete` receives completed tokens excluding the current partial one. Delete the "Check the flag" bullet. Cover both substitution forms in the fish-driven test from B2.

### S7. Broken pipes plus `panic = "abort"` give SIGABRT noise
* **Reviewer lens:** zoom-out, design, comprehension, implementation-readiness, codebase-details
* **Location:** lines 71-72, 145, 159
* **Problem:** Rust ignores SIGPIPE, so `bmk -l | head` or an early-exiting `fzf` makes writes return EPIPE. Any `unwrap`/`expect` on that path becomes an abort with status 134, an "Abort trap" message, and possibly a macOS crash report.
* **Fix:**
  - Treat `ErrorKind::BrokenPipe` on stdout as a quiet exit 0, and allow no `unwrap`/`expect` on I/O.
  - Use a locked `BufWriter` for `--ls`/`--tag`.
  - `--complete` never writes to stderr and always exits 0, apart from any B2 sentinel.
  - Add a test that pipes `--ls` into a reader that closes early.

### S8. The open questions are mostly already decided, and the one that is not affects the stdout contract
* **Reviewer lens:** all 7
* **Location:** lines 196-201, cross-referencing 32, 69, 72
* **Problem:**
  - Question 198 is decided at line 69 (fail; `-f` overwrites).
  - Question 199 is decided at line 72 (AND) and conflicts with S1.
  - Question 200 is decided at line 32, and "lowercase by convention" appears nowhere else.
  - Question 201 (bare `bmk`) is decided nowhere. If bare `bmk` lists to stdout, `cd "$(bmk)"` gets a multi-line string, which violates the one-path-or-nothing contract.
* **Fix:** delete questions 198-200 or mark them decided. Decide 201 now: bare `bmk` prints usage to stderr and exits 2. Add that as a CLI table row.

### S9. Contracts for `--rm`, `--check` and `--edit` are incomplete
* **Reviewer lens:** structural, implementation-readiness, comprehension, design, zoom-out, codebase-details
* **Location:** lines 70, 73-74, 84-91
* **Fix:**
  - `--rm` on a missing key exits 1. `--rm` on a namespace exits 1 ("is a namespace") and never deletes the subtree silently.
  - `--check` prints `path:line: message` on stderr. It exits 0 when clean and a defined non-zero code on findings (1, or a new code). List every check: duplicates, conflicts, bad segments, depth, uppercase tags, lines with no tab.
  - `--edit` tries `$VISUAL`, then `$EDITOR`, then `vi`. It handles values with arguments such as `code -w`, via whitespace split or `sh -c '$EDITOR "$1"' sh <file>`. It creates the parent directory and file if missing and passes the editor's exit status through. Optionally it runs `--check` afterwards.

### S10. First write, file permissions and concurrent writers are not covered
* **Reviewer lens:** codebase-details, zoom-out, design, implementation-readiness, comprehension, structural
* **Location:** lines 57-60
* **Problem:**
  - A missing file is an empty store, but the first `--add` fails if `~/.config/bmk/` does not exist.
  - Two concurrent read-modify-rename writes lose an update.
* **Fix:**
  - Call `fs::create_dir_all(parent)` before the first write.
  - On concurrency, either state "last writer wins, accepted for a single user", or take `File::lock` (std, stable since Rust 1.89) on a sidecar lock file around the read-modify-rename.

### S11. There is no test plan
* **Reviewer lens:** implementation-readiness, design, codebase-details
* **Location:** lines 136, 187-191
* **Problem:** "integration tests via BMK_FILE + temp dirs" lists no cases.
* **Fix:** list cases per milestone:
  - lookup: hit; miss (exit 1, empty stdout); namespace (exit 1, children on stderr); missing file (exit 1, not 3); `~` expansion; CRLF; a skipped malformed line;
  - add: duplicate, `-f`, namespace conflict in both directions, depth, `-`/dot segments, `.` with `$PWD` set and unset, relative paths, no trailing newline;
  - writes: comment and order preservation, symlinked `BMK_FILE`;
  - output: TTY vs piped `--ls`, broken pipe;
  - tags: AND, case folding;
  - commands: `--complete` outputs, `--check` exit codes.

  Specify the harness:
  - locate the binary with `env!("CARGO_BIN_EXE_bmk")`;
  - hand-roll temp dirs under `std::env::temp_dir()` with pid+counter names, and state whether "zero crates" covers dev-dependencies such as `tempfile`;
  - set `HOME`, `PWD` and `BMK_FILE` on every `Command` and remove `XDG_CONFIG_HOME`.

### S12. Unquoted-substitution advice understates the risk
* **Reviewer lens:** design, codebase-details, style
* **Location:** lines 102-104
* **Problem:** "worst case they open with no argument" is wrong for commands that default to the cwd. `find (bmk typo) -delete` becomes `find -delete` in the cwd. `ls`, `du`, `rm -r` and `cp x (bmk typo)` change meaning in the same way.
* **Fix:** say "unquoted is safe only when the command's no-argument behavior is harmless; quote everything else". Offer `set -l p (bmk x); and cmd $p` as the safe pattern.

### S13. Tag rules are incomplete
* **Reviewer lens:** style, comprehension, zoom-out, implementation-readiness, structural, design
* **Location:** lines 33, 72, 119
* **Problem:** "stored lowercase" does not say whether input is lowercased on add or on query, and a hand-edited file can contain uppercase tags.
* **Fix:**
  - Lowercase tags on add and on query.
  - Deduplicate tags, preserving order.
  - Empty tags (`a,,b`, `-t ""`) exit 2.
  - `--check` flags uppercase tags.
  - Tag completion after `-t docker,` prints `docker,<tag>` candidates so fish's prefix filter still matches.

### S14. `--ls` / `--tag` output is underspecified
* **Reviewer lens:** implementation-readiness, structural
* **Location:** lines 71-72
* **Problem:** the plan does not fix the output order, whether paths are expanded, the TTY column layout, or what an empty or unknown-subtree result exits with.
* **Fix:**
  - Print in file order.
  - Expand `~` in piped mode so `cut -f2` feeds `cd`, and choose one behavior for TTY mode.
  - In TTY mode, pad columns to the maximum width plus 2.
  - `--ls` with an empty result exits 0, and on an unknown subtree exits 1. Pick an exit code for `--tag` with no matches.

### S15. Lookup argument validation
* **Reviewer lens:** comprehension, zoom-out, implementation-readiness, design, structural
* **Location:** line 141
* **Problem:** joining args with `/` means `bmk personal/kotlin-app` (one token) also resolves. Invalid segments on lookup are unspecified.
* **Fix:** either document the single `/`-joined token as supported, which zoom-out notes would also simplify `--add` completion, or reject `/` inside an argument. Invalid segments on lookup exit 2.

### S16. Storage location edge cases
* **Reviewer lens:** implementation-readiness, design, codebase-details
* **Location:** lines 39-42, 60
* **Problem:** empty environment variables and a relative `$XDG_CONFIG_HOME` are not handled, and a mistyped `BMK_FILE` fails without explanation.
* **Fix:**
  - Treat an empty `$BMK_FILE` or `$XDG_CONFIG_HOME` as unset.
  - Ignore a relative `$XDG_CONFIG_HOME`, per the XDG spec.
  - Exit 3 if `$HOME` is needed and unset.
  - Consider a stderr warning when `BMK_FILE` is set but missing, since a typo currently gives a silent "not found".

### S17. `cargo clippy -D warnings` is invalid
* **Reviewer lens:** codebase-details, comprehension, design, implementation-readiness, structural
* **Location:** line 176
* **Problem:** cargo itself receives `-D` and rejects it, so CI fails on its first run.
* **Fix:** `cargo clippy --all-targets -- -D warnings`.

### S18. Inaccurate framing statements
* **Reviewer lens:** style, comprehension
* **Location:** lines 3-5, 14
* **Problem:** "prints a path to stdout and does nothing else" is false, because `--add`, `--rm` and `--edit` write files. "Any directory ... with any alias" overstates what the segment rules in line 32 allow.
* **Fix:** "A lookup prints one path to stdout and has no side effects. bmk installs no shell hook and no `cd` wrapper." Change line 14 to "any alias matching the segment rules".

---

## Nits

| # | Location | Problem | Fix | Lens |
|---|---|---|---|---|
| N1 | lines 17, 181 | Two different speed targets ("under 1 ms of work" vs "about 1 ms end to end"). Process spawn plus dyld on macOS is often around 1 ms by itself. | Set one measurable target, e.g. `hyperfine -N --warmup 10` mean under X ms on a 1,000-line file on a named machine. Optionally add an in-process target under about 100 us. `-N` needs hyperfine 1.13 or later. | style, comprehension, design, codebase-details |
| N2 | line 103 | The abbreviation has no example. | `abbr -a cdb --set-cursor 'cd "$(bmk %)"'`, and note that `--set-cursor` needs fish 3.6 or later. Otherwise drop it. | comprehension, codebase-details, impl-readiness, style |
| N3 | lines 108, 173 | The installed completion file goes stale after upgrades. | Install a file that contains only `bmk --fish \| source`, or note that the install step must be re-run. | comprehension |
| N4 | line 172 | The `<you>` placeholder is still in the URL. | Fill it in, or mark it as a placeholder. | 4 lenses |
| N5 | lines 156-162 | `opt-level = 3` is already the release default. `rust-version` is not declared, although `IsTerminal` needs 1.70. `[package]` is missing. | Drop or comment `opt-level`. Add `[package]` with `rust-version = "1.70"`, or 1.89 if using `File::lock`, and an empty `[dependencies]`. | codebase-details, impl-readiness |
| N6 | line 150 | It is not stated which stream gets the TTY check. | "`stdout().is_terminal()`". | codebase-details |
| N7 | line 108 | The `include_str!` path is relative to the source file. | `include_str!("../completions/bmk.fish")` from `src/complete.rs`. | codebase-details |
| N8 | lines 185-191 | Tag completion (M3) comes before tags exist (M4). CI arrives last. "Polish" hides real features. | Put Tags before Completion and move CI into M1. Rename M5. List the files and tests for each milestone. | style, impl-readiness |
| N9 | lines 140-145 | Hot-path micro-details (`No println!`) are implementation notes, and `path + "\n"` is pseudo-Rust. | Reduce to "single read, linear scan, first match, one `write_all` to locked stdout". | zoom-out, style |
| N10 | line 148 | The `clap` startup-cost rationale is unmeasured and redundant given std-only. | "hand-rolled over `env::args_os()` (std only)". | style |
| N11 | line 71 | Having both TTY-aligned and TSV output means two output paths. | Optional: always emit TSV and let users run `column -t`. Keep both if desired. | zoom-out |
| N12 | lines 47-52 | The tabs in the sample config are invisible. | State that the separators are literal TAB characters, and tag the fence as `text`. | structural |
| N13 | lines 69-77 | `-a, --add` reads as a literal comma. The `-h`/`-V` behavior cell is empty. "Hidden" is not defined. | Use `-a\|--add`. Fill the row with "print to stdout, exit 0". Define Hidden as "not listed in `--help`". | structural, style, impl-readiness |
| N14 | whole doc | The feature-spec standard expects Problem, Alternatives Considered and Risks & Mitigations sections. | Add a short Alternatives section (zoxide/CDPATH, TOML vs TSV, shell function vs binary, clap). Add Risks (B1-B3, the empty-substitution pitfall, concurrency). | structural |
| N15 | lines 126-150 | No core types or error enum are sketched. | Sketch `enum Cmd`, `struct Line<'a>`, `enum Error` with `exit_code()`, and name the module that owns the namespace-children scan. | impl-readiness |
| N16 | throughout | Undefined terms ("name", "prefix", "subtree", "alias"), contractions, and the title "Plan". | Define name and prefix in the Data model and use only defined terms. Optionally retitle to "bmk - Design (v1)". | style |

---

## Claims verified as accurate (no change needed)

These come from the codebase-details lens and are marked "confident":
- `env::args_os` and `fs::read` returning `Vec<u8>`.
- `rename(2)` is atomic within one POSIX filesystem.
- `IsTerminal` is stable since 1.70.
- `lto = "fat"`, `codegen-units = 1`, `panic = "abort"` and `strip = true` are all valid. `panic` does not affect `cargo test`.
- `cargo install --git` builds from the full checkout, so `include_str!` works.
- Fish:
  - An empty unquoted substitution yields zero arguments, and a bare `cd` goes to `$HOME`.
  - `$(...)` / `"$(...)"` exist since fish 3.4.
  - Quoted substitutions strip trailing newlines.
  - Unquoted substitutions split only on newlines, so paths with spaces are safe.
  - fish prefix-filters `-a` candidates itself, and `name\tdescription` output is supported.
  - `[2..]` slicing works.
- Building with native flags on CI runners is harmless, given that no artifacts are published (line 177).

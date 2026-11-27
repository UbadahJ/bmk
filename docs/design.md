# `bmk` — Final Implementation Plan (v1)

`bmk` is a small, pure-`std` Rust CLI that resolves scoped directory bookmarks to paths.

```fish
cd "$(bmk personal server-containers)"
code (bmk personal kotlin-app)
```

A lookup prints exactly one path to `stdout` (or nothing on failure) and has no side effects. `bmk` installs no shell `init` hook and no `cd` wrapper. All management commands sit behind an explicit `--cli` (short: `-c`) switch, so every non-flag word in default mode is unambiguously a bookmark segment.

---

## Problem Statement

Directory jumpers such as `zoxide` and `autojump` rely on frecency and physical filesystem paths, while shell aliases and `CDPATH` are flat and clutter quickly. Related projects often live in unrelated directory trees (for example, `~/Dev/Personal/Kotlin/<Project>` and `~/Dev/Embedded/Personal/<Project>`). `bmk` decouples logical organization from disk layout by grouping arbitrary directories under explicit namespaces with custom aliases, stored in one local configuration file.

---

## Goals & Non-Goals (v1)

### Goals
- **Scoped:** Map any directory under any namespace and custom alias without moving files on disk.
- **Tree-ready:** Keys are `/`-joined segment paths (`personal/kotlin-app`). v1 enforces 1–2 segments (`const MAX_DEPTH: usize = 2`); allowing deeper nesting (`personal/kotlin/game`) later requires raising `MAX_DEPTH` without changing the file format or CLI grammar.
- **Fast hot path:** Lookup and completion read one file and perform a single linear byte scan (target: within 0.2 ms of `true` via `hyperfine -N` on a 1,000-line file). Target paths are never `stat()`ed, canonicalized, or validated during lookup, listing, completion, writes, or `edit`.
- **Explicit on-demand health checks:** `bmk --cli doctor` validates both config integrity and on-disk target directories when explicitly invoked.
- **Zero dependencies:** Pure `std` Rust (`edition = "2021"`, `rust-version = "1.70"`), including tests (no dev-dependencies).
- **Fish-native completions:** Fish (`>= 3.4`) is the primary target shell and is assumed installed on dev machines. Completions cover namespaces, names, management commands, flags, directories (on `add`), and tags, including inside `(...)` and `"$(...)"`.
- **Tag filtering:** Attach tags to bookmarks and filter by all specified tags (AND).

### Non-Goals (v1)
- Fuzzy matching or frecency scoring.
- Shell `init` hooks or `cd` wrappers.
- Shells other than Fish (minimum supported: Fish 3.4+).
- Windows support or non-UTF-8 paths/tags.
- Symlinked config files (bookmarks file is a regular, machine-local file).
- Keys deeper than `MAX_DEPTH = 2`.
- Auto-pruning (`doctor --prune`) or interactive TUI.
- Concurrent multi-writer locking (single-user local CLI; last writer wins).
- CI pipelines or prebuilt release binaries (built and tested locally per dev machine via `./install.sh`).

---

## Alternatives Considered

| Option | Why Rejected |
| :--- | :--- |
| `zoxide` / `autojump` | Matches against frecency and real folder paths; no explicit logical namespaces. |
| `CDPATH` / Fish `abbr` / symlink tree | Flat or leaks symlink paths into `$PWD`; symlinks embed absolute paths and require filesystem clutter. |
| `markd`, `fish-marks`, `bookmark-paths` | Flat namespaces and/or rely on shell wrapper functions. *(Local-only install into `~/.cargo/bin/bmk`, so external package naming does not apply.)* |
| Shell wrapper function for `cd` | Violates pure-resolver composition (`code (bmk ...)`, pipes); Fish 3.4+ `cd "$(bmk ...)"` avoids `$HOME` jumps without a wrapper. |
| TOML or RFC-4180 CSV config | TOML requires external crates and awkward mixed-depth tables; RFC-4180 CSV adds quoting/unescaping complexity purely for rare commas in paths. TSV is zero-dependency, greppable, and keeps tags comma-separated. |
| External crates (`clap`, `csv`, `dirs`) | Unnecessary compile/dependency footprint for a ~500-line tool meant to compile untouched for years. |
| Separate `check` + `doctor` commands | Redundant: checking 100+ local paths via `stat()` takes milliseconds, so one explicit `doctor` command covers both config lint and target directory checks. |

---

## Terms & Data Model

| Term | Meaning |
| :--- | :--- |
| **segment** | One name component (e.g., `personal`). Allowed: `[A-Za-z0-9._-]+`, must not start with `-`, must not consist solely of `.` (`.` / `..`), case-sensitive. |
| **key** | `1..=MAX_DEPTH` segments joined by `/` (e.g., `personal/kotlin-app`, `dotfiles`). |
| **bookmark** | A key mapped to a path and optional tags (one line in the config file). |
| **namespace** | An implicit key prefix that has descendant bookmarks (`personal` when `personal/app` exists); never stored separately. |
| **prefix** | A key or partial hierarchy used to select a subtree in `ls` (segment-boundary match). |
| **tag** | `[a-z0-9._-]+`, not starting with `-`, lowercased on input, comma-separated, deduplicated preserving order. |

- **Strict tree invariant:** A key is either a bookmark or a namespace, never both. Conflicting additions/renames (adding `personal/a/b` when `personal/a` is a bookmark, or adding `personal/a` when `personal/a/b` exists) are rejected with exit `1`, even with `-f`.
- `const MAX_DEPTH: usize = 2;` is enforced on `add`, `mv`, and `doctor`. Lookup, `ls`, and `__complete` operate at arbitrary depth.

---

## Storage

### Location (Hardcoded, No Overrides)
Always **`$HOME/.config/bmk/bookmarks`** (on macOS and Linux). No `$BMK_FILE`, `$XDG_CONFIG_HOME`, or CLI flag overrides. Keeping bookmarks inside `~/.config/bmk/` keeps atomic temp files contained in a dedicated directory and matches standard `~/.config/<tool>/` layout.

- The bookmarks file is machine-local (not shared across machines with differing directory trees) and is a regular file (symlinks are not supported).
- A missing `$HOME/.config/bmk/bookmarks` counts as an empty store for reads (`ls`, `tags`, `doctor`, `__complete`) and a normal miss (`exit 1`) for lookup/`rm`/`mv`/`tag`.
- If `$HOME` is unset or empty, exit `3`.
- Integration and Fish tests isolate runs by setting `HOME` to a temporary directory on the child process.

### Format (TSV)
UTF-8 text, one bookmark per line, separated by **literal TAB (`\t`) characters**:

```text
# key<TAB>path<TAB>tags (optional)
personal/server-containers	~/Dev/Embedded/Personal/server-containers	docker,infra
personal/kotlin-app	~/Dev/Personal/Kotlin/kotlin-app	android,kmp
dotfiles	~/dotfiles
acme/api	~/Work/acme/api
```

- Blank lines and lines whose first byte is `#` are ignored.
- A leading UTF-8 BOM (`\u{FEFF}`) on line 1 and trailing `\r` (CRLF) are stripped.
- Fields: `key\tpath` (2 fields, no tags) or `key\tpath\ttags` (3 fields; empty third field means no tags). More than 3 fields is malformed.

### `~` Handling
- **On write:** Normalize `$HOME` by stripping trailing `/` (except when `$HOME == "/"`). If `$HOME != "/"` and the path equals `$HOME` or starts with `$HOME/`, rewrite that prefix to `~`. Paths like `/Users/alice2` are never rewritten to `~2`.
- **On output / `doctor`:** Expand only exact `~` or leading `~/`. Never expand `~user`.

### Read Leniency vs. Write Strictness
- **Reads** (lookup, `ls`, `tags`, `__complete`): Lenient. Skip malformed lines (missing tab, invalid key, empty path, >3 fields). Exit `3` occurs only on I/O errors or missing `$HOME`.
- **Writes** (`add`, `rm`, `mv`, `tag`): Strict on syntax/schema (never `stat()` target paths). If the existing file has malformed lines, invalid keys, duplicates, namespace conflicts, empty tag elements (`a,,b`), or illegal tag characters (outside `[A-Za-z0-9._-]`), abort with exit `3`, print `<file>:<line>: <reason>` to `stderr`, suggest `bmk --cli doctor`, and write nothing. Hand-typed **uppercase tags** (e.g. `Docker`) do **not** block writes; they are matched case-insensitively on read and flagged only by `doctor`.

### Atomic Writes
1. Read and strictly validate the existing file (if present).
2. Build updated contents in memory: edit/remove lines in place, append new entries (inserting `\n` if the existing file lacks a trailing newline). Comments, blank lines, and manual ordering are preserved byte-for-byte.
3. Ensure the config file's parent directory exists via `fs::create_dir_all(parent)`.
4. Write to a sibling temp file `.<filename>.tmp.<pid>.<seq>` (monotonic `AtomicUsize` counter) using `OpenOptions::new().write(true).create_new(true)`. Copy existing file permissions if the original exists, write all bytes, and `sync_all()`.
5. Atomically `fs::rename` the temp file over the original. On any error before rename succeeds, remove the temp file.

---

## CLI Interface

### Mode Switch

```text
bmk <segment>...           lookup mode (default)
bmk --cli <command> ...    management mode (short form: -c)
bmk -h | --help            usage to stdout, exit 0
bmk -V | --version         version to stdout, exit 0
bmk                        usage to stderr, exit 2 (keeps `cd "$(bmk)"` safe)
```

Because segments cannot start with `-`, top-level flags never collide with bookmark names. Any unknown `-...` argument in lookup mode exits `2`.

### Lookup Mode
`bmk <segment>...` — each argument may also contain `/`, so `bmk personal kotlin-app` and `bmk personal/kotlin-app` are equivalent. Arguments are split on `/`, validated (invalid segment exits `2`), and joined with `/`.

| Result | `stdout` | `stderr` | Exit |
| :--- | :--- | :--- | :--- |
| Bookmark matches (first match wins) | `<expanded_path>\n` | — | `0` |
| Key is a namespace | — | `bmk: '<key>' is a namespace: <child1>, <child2>` | `1` |
| Not found (or missing file) | — | `bmk: no bookmark '<key>'` | `1` |

### Management Commands (`bmk --cli <cmd>` / `bmk -c <cmd>`)
Management commands take a **single `/`-joined `<key>` token**, keeping `add <key> <path>` unambiguous at any depth (including once `MAX_DEPTH` is raised).

| Command | Behavior |
| :--- | :--- |
| `add <key> <path> [-t\|--tags <csv>] [-f\|--force]` | Adds a bookmark (`<path>` `.` = cwd), echoing the stored line to `stderr`. Existing key exits `1` unless `-f` replaces that line in place (including tags). Always exits `1` on namespace/bookmark conflict or depth `> MAX_DEPTH`. |
| `rm <key>` | Removes an exact bookmark. Missing key or namespace (`is a namespace`) exits `1`; subtrees are never deleted. |
| `mv <key> <new-key>` | Renames a bookmark in place, preserving line position, path, and tags. Same conflict/depth rules as `add`; no `-f`. |
| `tag <key> (-t\|--tags <csv> \| --clear)` | Replaces the bookmark's tags or clears them. Missing key exits `1`. |
| `ls [<prefix>] [-t\|--tags <csv>]` | Lists bookmarks in file order, optionally scoped to `<prefix>` (segment-boundary match) and/or matching **all** given tags (AND, case-insensitive). |
| `tags` | Lists all tags with usage counts, sorted alphabetically by tag name. |
| `edit` | Picks `ed` from `$VISUAL` (if non-empty), then `$EDITOR` (if non-empty), then `vi`, creating parent dir and file if missing. Runs `sh -c "$ed \"\$1\"" sh <file>` (so values with args like `code -w` work and `<file>` is passed safely as `$1`). Passes the editor's exit status through. Runs syntax/schema validation only (never target `stat()`) afterwards and prints any file findings to `stderr` without overriding the exit status. |
| `doctor` | Explicit on-demand health check: validates config syntax/schema and checks every bookmark's target directory on disk in one read-only pass (see **Doctor Rules** below). Exits `0` if clean, `1` if findings exist, `3` on I/O or `$HOME` error. |
| `path` | Prints the resolved config path to `stdout` (plus a note on `stderr` if the file does not exist yet). |
| `fish` | Prints the embedded Fish completion script to `stdout`. |
| `help` / `version` | Same as `-h` / `-V`. |
| `__complete <current> <token>...` | Hidden internal completion protocol (omitted from `help`). |

### Management Grammar Rules
- `--cli` / `-c` must be the first argument, followed by `<command>`.
- `-h` or `--help` after any management command (e.g., `bmk -c add -h`) prints that command's usage to `stdout` and exits `0`.
- Flags and positionals may be mixed in any order; `--` terminates flag parsing (e.g., for a directory named `-foo`).
- Value-taking flags consume the next argument (`-t a,b`); no `--tags=a` and no bundled short flags (`-tf`).
- Unknown flags, missing/extra positionals, or empty `-t` values exit `2` with usage on `stderr`.
- After `__complete`, all remaining tokens are raw data and never parsed as flags.

### Path Normalization on `add`
1. Reject `\t`, `\n`, `\r`, and non-UTF-8 bytes in paths and tags (exit `2`).
2. If `<path>` is `~` or starts with `~/`, expand `$HOME` first (so single-quoted `'~/foo'` works).
3. If `<path>` is relative, join onto `$PWD` (when set and starting with `/`, preserving shell symlinks), falling back to `env::current_dir()`.
4. Lexically normalize without touching the target filesystem: collapse `//`, drop `.`, resolve `..` against the preceding segment (clamping at `/`), strip trailing `/` (preserving bare `/`).
5. Apply `~` contraction.

### Tag Rules
- Comma-separated everywhere (`-t a,b`). On input, tags are lowercased and duplicates are removed preserving order; an empty element (`a,,b`) or character outside `[A-Za-z0-9._-]` exits `2`.
- `ls -t a,b` requires **all** given tags (AND), matched case-insensitively so hand-edited uppercase tags still match and do not block writes.

### `doctor` Rules (`bmk --cli doctor`)
Strictly read-only; run explicitly only (never invoked automatically by `edit` or other commands). Checks every non-blank, non-comment line and reports all findings to `stderr` as `<file>:<line>: <reason>`:
1. **Syntax:** Missing tab, empty path, or more than 3 tab-separated fields.
2. **Keys:** Invalid segment characters, depth `> MAX_DEPTH`, duplicate keys, or namespace/bookmark prefix conflicts.
3. **Tags:** Empty tag items (`a,,b`), invalid characters, uppercase tags, or duplicate tags on the same line.
4. **Target paths (on-disk):** Because the bookmarks file is machine-local, missing targets are errors. For each syntactically valid bookmark, expand `~` and check `fs::metadata(&expanded)` (follows symlinks):
   - Target does not exist (or broken symlink): `<file>:<line>: target does not exist: <path>`
   - Target exists but is not a directory: `<file>:<line>: target is not a directory: <path>`
- Missing config file: exits `0` with no findings.
- Any finding: exits `1`, empty `stdout`. Clean: exits `0`, empty `stdout`.

### `ls` and `tags` Output
- **Interactive TTY** (`stdout().is_terminal()`): Aligned columns (`key`, `path` keeping `~`, `tags`; each column padded to widest value + 2 spaces). `tags` aligns `<tag>` and `<count>`.
- **Piped (non-TTY):** Raw TSV `key\tpath\ttags` with `~` **expanded** so `bmk -c ls | fzf | cut -f2` yields a usable path. `tags` emits `<tag>\t<count>`.
- **Exit codes:** `0` normally (including empty store with no filters); `1` if `<prefix>` matches nothing or `-t` matches nothing.
- Output uses a locked `BufWriter`; `ErrorKind::BrokenPipe` (`EPIPE`) exits `0` silently.

### Exit Codes

| Code | Meaning |
| :--- | :--- |
| `0` | Success (including `EPIPE` on `stdout`) |
| `1` | Not found, namespace where bookmark expected, conflict, `doctor` findings, or `ls` filter with no matches |
| `2` | Usage error or invalid input |
| `3` | Config/I/O error, strict-parse failure before a write, or `$HOME` unset/empty |

On any non-zero exit, `stdout` stays **empty** and diagnostics go to `stderr`. Production I/O paths use zero `unwrap()`/`expect()` calls so `panic = "abort"` never turns a runtime error into `SIGABRT`.

---

## Fish Usage & Completions

### Empty-Substitution Safety Note
In Fish, an unquoted command substitution with empty `stdout` expands to **zero arguments**, turning `cd (bmk typo)` into bare `cd` (`$HOME`). Exit codes do not prevent this.
- Always quote `cd`: `cd "$(bmk personal app)"` (Fish 3.4+; on failure `cd ""` exits non-zero and leaves `$PWD` unchanged).
- Optional abbreviation (Fish 3.6+): `abbr -a cdb --set-cursor 'cd "$(bmk %)"'`.
- For destructive commands, quote or guard: `set -l p (bmk x); and cmd "$p"`.

### Static Completion Script (`completions/bmk.fish`)
Embedded at compile time via `include_str!("../completions/bmk.fish")` (from `src/complete.rs`) and printed by `bmk --cli fish`. `./install.sh` writes a one-line stub `command -q bmk; and command bmk --cli fish | source` to `~/.config/fish/completions/bmk.fish` so completions always match the installed binary.

```fish
function __bmk_complete
    # -x is preferred in Fish 4.0+; fall back to -o on Fish 3.4+.
    set -l tokens (commandline -xpc 2>/dev/null; or commandline -opc)
    set -l current (commandline -ct)
    set -l out (bmk --cli __complete "$current" $tokens[2..])
    if test "$out[1]" = __BMK_DIRS__
        __fish_complete_directories "$current"
    else
        printf '%s\n' $out
    end
end
complete -c bmk -f -a '(__bmk_complete)'
```

### `__complete` Protocol
- **Input:** `<current>` (partial token under cursor, possibly `""`), followed by completed tokens after `bmk`.
- **Output:** `candidate\tdescription` lines (Fish performs prefix filtering), or the single line `__BMK_DIRS__` to trigger Fish directory completion.
- **Safety:** Always exits `0`, never writes to `stderr`, skips malformed lines, and treats a missing file as empty.

| Position | Candidates |
| :--- | :--- |
| `bmk ▮` | Top-level segments (`<path>` or `namespace`). Adds `--cli`, `-c`, `-h`, `-V` when `<current>` starts with `-`. |
| `bmk personal ▮` | Children under `personal` (none below a leaf bookmark). |
| `bmk --cli ▮` | Management commands (`add`, `rm`, `mv`, `tag`, `ls`, `tags`, `edit`, `doctor`, `path`, `fish`, `help`, `version`) with one-line descriptions. |
| `add ▮`, `rm ▮`, `mv ▮`, `tag ▮`, `ls ▮` | `rm`/`mv`/`tag`: bookmark keys; `add`: namespace prefixes `ns/`; `ls`: namespace prefixes and bookmark keys. |
| `add <key> ▮` | `__BMK_DIRS__` |
| `mv <key> ▮` | Namespace prefixes `ns/` |
| After `-t` / `--tags` | Known tags (if `<current>` is `docker,`, emits `docker,<tag>` excluding already-listed tags). |
| `<current>` starts with `-` (in management cmd) | Valid flags for that command with descriptions. |

---

## Proposed Architecture & File Layout

### File Layout (as implemented)

```text
bmk/
├── Cargo.toml              # lib + bin, no dependencies, release profile
├── Cargo.lock
├── build.rs                # sets BMK_BUILD=native|generic for `bmk -V`
├── .cargo/config.toml      # target-cpu=native for local builds
├── install.sh              # native install, fish completion stub, config init, --uninstall
├── scripts/bench.sh        # reproduces docs/benchmark.md
├── completions/bmk.fish    # fish completion script, embedded in the binary
├── docs/
│   ├── design.md           # this document
│   ├── benchmark.md        # measured performance and analysis
│   └── design-review.md    # EGM review of an earlier draft
├── src/
│   ├── main.rs             # binary: forwards args to bmk::main
│   ├── lib.rs              # mode switch (lookup vs --cli), top-level dispatch
│   ├── error.rs            # Error, exit codes, stderr messages
│   ├── output.rs           # write_stdout (EPIPE-safe), note
│   ├── lookup.rs           # lookup mode
│   ├── key.rs              # segment/key/tag rules, MAX_DEPTH, ~ handling, path cleaning
│   ├── complete.rs         # __complete protocol, FISH_SCRIPT
│   ├── cli/
│   │   ├── mod.rs          # management-mode parser, Cmd
│   │   └── help.rs         # all usage/help/version texts
│   ├── commands/
│   │   ├── mod.rs          # dispatch + shared helpers (Ctx, conflict checks)
│   │   └── add.rs, rm.rs, mv.rs, tag.rs, ls.rs (ls + tags), edit.rs, doctor.rs, path.rs
│   └── store/
│       ├── mod.rs          # location, read, lenient scanner, lookup
│       ├── check.rs        # strict validation (Finding, check, lines_for_write)
│       ├── doc.rs          # line-preserving edits (Doc, format_line)
│       └── write.rs        # atomic write
└── tests/
    ├── common/mod.rs       # Env/Out harness (temporary HOME per test)
    ├── lookup.rs, manage.rs, list.rs, edit_doctor.rs, complete.rs
```

### Core Types

```rust
pub enum Cmd {
    Lookup(Vec<String>),
    Add { key: String, path: String, tags: Vec<String>, force: bool },
    Rm(String),
    Mv { old_key: String, new_key: String },
    Tag { key: String, tags: Option<Vec<String>> }, // None = --clear
    Ls { prefix: Option<String>, tags: Vec<String> },
    Tags,
    Edit,
    Doctor,
    Path,
    Fish,
    Complete { current: String, tokens: Vec<String> },
    Help(Option<&'static str>), // None = top-level help, Some(cmd) = per-command usage
    Version,
}

pub struct Line<'a> {
    pub no: usize,
    pub key: &'a [u8],
    pub path: &'a [u8],
    pub tags: &'a [u8],
}

pub enum Error {
    NotFound(String),
    IsNamespace(String, Vec<String>),
    Conflict(String),
    DoctorFailed,
    NoMatches,
    Usage(String),
    Invalid(String),
    Config(String),
    Io(std::io::Error),
}

impl Error {
    pub fn exit_code(&self) -> i32 {
        match self {
            Error::NotFound(_)
            | Error::IsNamespace(_, _)
            | Error::Conflict(_)
            | Error::DoctorFailed
            | Error::NoMatches => 1,
            Error::Usage(_) | Error::Invalid(_) => 2,
            Error::Config(_) | Error::Io(_) => 3,
        }
    }
}
```

### Hot Path (`store.rs`)
1. Join validated lookup segments into target key bytes.
2. Read the entire config file once with `fs::read`.
3. Scan lines as `&[u8]` without allocating a map: return immediately on first exact key match, while tracking direct child names if a line starts with `<target>/`.
4. Expand `~` only on the matched path and emit `<path>\n` via a single `write_all` on locked `stdout`.

### Build & Local Install
`Cargo.toml`:
```toml
[package]
name = "bmk"
version = "0.1.0"
edition = "2021"
rust-version = "1.70"

[dependencies]

[profile.release]
lto = "fat"
codegen-units = 1
panic = "abort"
strip = true
```

`.cargo/config.toml`:
```toml
[build]
rustflags = ["-C", "target-cpu=native"]
```

`build.rs` inspects `CARGO_ENCODED_RUSTFLAGS` so `bmk -V` prints `bmk 0.1.0 (native)` or `bmk 0.1.0 (generic)`. Installation on each dev machine runs `./install.sh` (which sets `RUSTFLAGS` explicitly, builds natively, writes the Fish stub, initializes the bookmarks file, and smoke-tests `bmk --version`). No CI workflow or distributed release binaries are used.

---

## Performance Target (Must Have)

- **Lookup target:** `hyperfine -N --warmup 10 'bmk personal kotlin-app'` (`hyperfine >= 1.13`) on a 1,000-line bookmarks file must have a mean latency within **0.2 ms** of `hyperfine -N 'true'` on the same machine (i.e., `bmk` adds negligible work beyond OS process spawn).
- **Completion target:** `hyperfine -N --warmup 10 "bmk --cli __complete '' personal"` on the same 1,000-line file must also be within **0.2 ms** of `true`.
- **Out of scope:** caching, indexing, `mmap`, custom allocators, PGO.

---

## Risks & Mitigations

| Risk | Mitigation |
| :--- | :--- |
| Unquoted `cd (bmk typo)` jumps to `$HOME` | Document `cd "$(bmk ...)"`, safe `set -l p (bmk ...); and ...`, and optional Fish `abbr`. |
| Editor with `expandtab` turns tabs into spaces | Reads skip malformed lines; writes refuse to overwrite; `edit` runs syntax/schema validation immediately after closing. |
| Build silently misses `-C target-cpu=native` | `./install.sh` passes `RUSTFLAGS` explicitly and `bmk -V` reports `(native)` vs `(generic)`. |
| Concurrent writes drop an update | Accepted for a single-user local CLI (last writer wins). |
| Fish 3.4–3.7 lacks `commandline -x` | Completion script tries `commandline -xpc` and falls back to `commandline -opc`. |

---

## Phased Implementation Milestones & Verification

All development and testing runs locally with **Fish (`>= 3.4`) installed**. Each milestone ends with `cargo test` passing and clean `cargo clippy --all-targets -- -D warnings`.

1. **Phase 1 — Scaffold, Installer Sync, Key/Path Logic, and Lookup Hot Path:**
   - Create `Cargo.toml`, `.cargo/config.toml`, `build.rs`, `src/key.rs`, `src/store.rs` (location + lenient byte scan), and `src/main.rs` (mode switch, lookup, `-h`/`-V`, `EPIPE`, `Error::exit_code`). Update `install.sh` so `bookmarks_file="$HOME/.config/bmk/bookmarks"` (dropping `BMK_FILE`/XDG for bookmarks while keeping `$XDG_CONFIG_HOME` for Fish completions).
   - *Checkpoint:* Unit + integration tests (isolated via `HOME=<tempdir>`) for lookup hits/misses, `a b` == `a/b`, namespace error listing children, BOM/CRLF, malformed line skipping, `~` boundary rules, `$HOME/.config/bmk/bookmarks` location, unset/empty `$HOME` (exit `3`), bare `bmk` exit `2`.
2. **Phase 2 — Management Commands & Atomic Writes:**
   - Implement `src/cli.rs` (including per-command `-h`/`--help`), strict store parser + atomic writes in `src/store.rs`, and `add`/`rm`/`mv`/`path` in `src/cmd.rs`.
   - *Checkpoint:* Integration tests for `add` (`.` with `$PWD` set/unset, `..` cleaning, `-f`, conflicts both ways, depth 3, tab/newline rejection, missing `~/.config/bmk/` dir creation, no trailing newline, comment/order/permissions preservation, uppercase tags do NOT block writes, malformed file blocks writes with exit `3`), `rm`, `mv`, `path`, and per-command `-h`/`--help` (exit `0`).
3. **Phase 3 — Tags, Listing, `edit`, and `doctor`:**
   - Implement `tag`, `ls`, `tags`, `edit`, and `doctor` in `src/cmd.rs`.
   - *Checkpoint:* Integration tests for TTY vs. piped (`~` expanded) `ls`/`tags`, segment-boundary prefix filtering, multi-tag AND + case-insensitive matching, `EPIPE` exit `0`, `edit` using `$VISUAL` then `$EDITOR` (with args and exit passthrough, syntax check only—no target `stat()`), and explicit `doctor` covering clean store (`0`), malformed lines, bad keys/duplicates/conflicts/depth/uppercase tags, missing target directory, regular file target, and symlinked valid/broken directory (`1` with `<file>:<line>`).
4. **Phase 4 — Fish Completions & Fish-Driven Tests:**
   - Create `completions/bmk.fish` and `src/complete.rs`.
   - *Checkpoint:* Protocol tests for every row in the `__complete` table plus Fish-driven tests (requiring `fish` on `PATH`: `env PATH=... HOME=<tempdir> fish -c "source completions/bmk.fish; complete -C '...'"`) covering standalone `bmk`, `cd "$(bmk personal `, and `bmk --cli add personal/x ` directory completion.
5. **Phase 5 — Docs, Lockfile, Native Release & Performance Benchmark:**
   - Write `README.md`, ensure `Cargo.lock` is generated and up to date, run `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`, `cargo build --release`, verify `bmk -V` reports `(native)`, and benchmark lookup & `__complete` on a 1,000-line file with `hyperfine -N` against `true` (must be within 0.2 ms).

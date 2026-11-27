# Benchmarks

How fast is `bmk`, where does the time go, and would anything else be faster?

**Short answer:** a lookup takes about 2.1 ms on an M1 Max, and almost all of that is macOS
starting the process. bmk's own work (reading and scanning a 1,001-line file) is about
0.1 ms. Rewriting it in another language or adding a daemon wouldn't make it noticeably
faster. The only way below process-start cost is to resolve inside the shell, which
conflicts with bmk's design.

To reproduce everything here, run `scripts/bench.sh` (see [Reproducing](#reproducing)).

## Setup

| | |
|---|---|
| Machine | Apple M1 Max, 10 cores, 64 GB |
| OS | macOS 27.2.1 |
| Toolchains | rustc 1.85.1, Apple clang 21.0.0, Swift 6.4.2, fish 4.9.3 |
| Tool | hyperfine 2.0.0: `-N` (no shell), 100 warmup runs, 3,000 timed runs, medians |
| bmk build | `--release` with `lto = "fat"`, `codegen-units = 1`, `panic = "abort"`, `target-cpu=native` |
| Data | A temporary `HOME` with a 1,001-line bookmarks file; the bookmark looked up is on the last line, so the scan is worst case |

The baselines are the smallest possible programs in each language, built on the same
machine. They do nothing except what their name says.

## Results

### bmk against minimal programs

| Command | Median | vs `true` | vs empty Rust |
|---|---:|---:|---:|
| `/usr/bin/true` | 1.130 ms | — | −0.768 ms |
| empty C program | 1.588 ms | +0.459 ms | −0.310 ms |
| Swift, standard library only | 1.735 ms | +0.606 ms | −0.163 ms |
| empty Rust program | 1.898 ms | +0.768 ms | — |
| `bmk -V` (doesn't read the file) | 2.046 ms | +0.916 ms | +0.148 ms |
| **`bmk personal kotlin-app`** | **2.149 ms** | +1.020 ms | +0.251 ms |
| **`bmk --cli __complete '' personal`** | **2.197 ms** | +1.067 ms | +0.299 ms |
| Objective-C + Foundation | 4.242 ms | +3.112 ms | +2.344 ms |
| Swift + Foundation | 4.349 ms | +3.219 ms | +2.451 ms |

Medians vary by about ±0.1 ms between runs. In one earlier run, `true` measured 1.00 ms and
the lookup 2.05 ms; the gaps between rows stayed the same.

### Where a lookup's 2.15 ms goes

| Part | Time | How it's measured |
|---|---:|---|
| Starting any process on macOS | ≈1.1 ms | `/usr/bin/true` |
| Extra cost for a binary built locally, not shipped with macOS | ≈0.45 ms | empty C − `true` |
| Rust's runtime and a larger binary | ≈0.3 ms | empty Rust − empty C |
| bmk's own startup (more code to load) | ≈0.15 ms | `bmk -V` − empty Rust |
| **bmk's actual work:** open, read and scan 1,001 lines, print | **≈0.10 ms** | lookup − `bmk -V` |

macOS launches its own system binaries faster than anything you build yourself.
Differences in binary size and runtime account for the next few tenths of a
millisecond. The part bmk controls directly is the last row.

### Inside fish: no process at all

The same lookup, 300 calls in one fish process. These timings include the test loop's
own overhead.

| How the lookup runs | Per call |
|---|---:|
| `bmk` binary, which starts a process each call | 2.296 ms |
| fish function that splits each line while scanning | 20.825 ms |
| fish function over lists loaded into the shell beforehand | 0.277 ms |

## What this means

- **Other languages.** A Foundation-based Swift or Objective-C version is about twice as
  slow: loading Foundation and the Objective-C runtime adds about 2.4 ms per call. Swift
  using only its standard library starts about 0.16 ms faster than an empty Rust program,
  because macOS already has the Swift runtime loaded, but it lacks a real file and byte
  API, and on Linux the runtime is a separate install. C is the floor for a binary built
  locally, about 0.5 ms faster than bmk today.
- **Trimming Rust.** The ~0.45 ms between bmk and empty C is Rust's runtime plus bmk's
  extra code. Going `no_std` with raw libc calls could recover most of it, at the cost of
  losing `std`'s file and environment APIs. That isn't worth it for a tool you call on
  `cd` or a tab press.
- **A daemon can't help.** The shell still has to start a client process to talk to it,
  which is the expensive part. A daemon would remove only the ~0.1 ms of file work and
  add a socket round trip, so it would be slightly slower overall.
- **Inside the shell is the only way below 1 ms.** A fish function over preloaded lists
  answers in under 0.3 ms. But the bookmarks then have to be loaded when the shell starts
  (an init hook), they go stale until reloaded, and the lookup works only in fish. All of
  that goes against bmk's design. Doing it naively, by splitting each line as you scan,
  is 10× slower than the binary.
- **Perception.** One 60 Hz frame is 16.7 ms, and people start noticing delay at around
  50–100 ms. At 2 ms per call, bmk is well below that.

### The design target

`docs/design.md` sets the target as "within 0.2 ms of `true`". On macOS that can't be met
by any binary built locally, including an empty C program (+0.46 ms). A target bmk can
control is "lookup adds at most 0.2 ms over `bmk -V`". It currently adds about 0.10 ms.

## Linux (estimates, not measured)

No Linux machine or container was available, so these are typical figures:

| | macOS (measured) | Linux (typical) |
|---|---:|---:|
| Start a minimal process (`true`) | 1.1 ms | ~0.3–0.5 ms |
| bmk lookup, dynamically linked | 2.15 ms | ~0.4–0.8 ms |
| bmk lookup, static musl build | — | ~0.2–0.4 ms |
| bmk's own work | ~0.1 ms | about the same |

Process start on Linux is cheaper for three reasons:

- No code-signature checks at launch.
- A statically linked binary (`--target x86_64-unknown-linux-musl` or
  `aarch64-unknown-linux-musl`) needs no dynamic loader.
- The kernel's process creation is cheaper.

musl's slower memory allocator doesn't matter here, because bmk allocates almost nothing.
Run `scripts/bench.sh` on a Linux machine to replace these estimates with measurements.

## Reproducing

```sh
scripts/bench.sh          # 3,000 runs per command
scripts/bench.sh 500      # quicker, noisier
```

The script:

1. Builds bmk in release mode.
2. Creates a temporary `HOME` with the 1,001-line file.
3. Builds the baseline programs. Rows whose compiler is missing are skipped, and the
   Objective-C row runs on macOS only.
4. Runs hyperfine and prints the first table.
5. If fish is installed, runs the in-shell comparison.

It needs `cargo`, `hyperfine` and `python3`. Your real `~/.config` is never touched, and the
temporary files are removed on exit.

Keep in mind:

- Close busy apps. Background load shifts every row.
- Call `true` by its full path (`/usr/bin/true`). When `/bin/sh` resolves the bare name
  `true`, one run measured 1.87 ms instead of ~1.1 ms.
- Compare the gaps between rows rather than absolute times, which drift by ~0.1 ms
  between runs.

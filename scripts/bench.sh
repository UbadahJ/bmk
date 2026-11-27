#!/bin/sh
# Reproduces docs/benchmark.md.
#
# Usage: scripts/bench.sh [runs]          (default: 3000 runs per command)
#
# Needs: cargo, hyperfine, python3. Optional: cc/clang, swiftc, fish (those rows are
# skipped when the tool is missing). Everything runs against a temporary HOME holding a
# 1,001-line bookmarks file; your real ~/.config is never touched.
set -eu

runs=${1:-3000}
root=$(cd "$(dirname "$0")/.." && pwd)
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT INT TERM

for tool in cargo hyperfine python3; do
    command -v "$tool" >/dev/null 2>&1 || { echo "error: $tool not found" >&2; exit 1; }
done

echo "==> Building bmk (release)"
(cd "$root" && cargo build --release -q)
bmk="$root/target/release/bmk"

echo "==> Preparing a temporary HOME with 1,001 bookmarks"
mkdir -p "$work/home/.config/bmk"
python3 - "$work/home/.config/bmk/bookmarks" <<'EOF'
import sys
with open(sys.argv[1], "w") as f:
    for i in range(1000):
        f.write(f"ns{i % 40}/project-{i}\t~/Dev/some/where/project-{i}\ttag{i % 7},x\n")
    f.write("personal/kotlin-app\t~/Dev/Personal/Kotlin/kotlin-app\tandroid\n")
EOF

echo "==> Building baseline binaries"
cmds="$work/cmds"
: > "$cmds"
row() { printf '%s\t%s\n' "$1" "$2" >> "$cmds"; }

row "true" /usr/bin/true
printf 'fn main(){}\n' > "$work/e.rs"
rustc -O -C panic=abort -C strip=symbols "$work/e.rs" -o "$work/empty_rust"
row "empty Rust" "$work/empty_rust"
if command -v cc >/dev/null 2>&1; then
    printf 'int main(void){return 0;}\n' > "$work/e.c"
    cc -O2 "$work/e.c" -o "$work/empty_c" && row "empty C" "$work/empty_c"
fi
if command -v swiftc >/dev/null 2>&1; then
    printf 'let a = CommandLine.arguments\nvar s = [String]()\nfor x in a { s.append(x.uppercased()) }\nif s.count > 99 { print(s) }\n' > "$work/s.swift"
    swiftc -O "$work/s.swift" -o "$work/swift_stdlib" 2>/dev/null && row "Swift stdlib only" "$work/swift_stdlib"
    printf 'import Foundation\nlet p = FileManager.default.homeDirectoryForCurrentUser.path\nif p.isEmpty { print(p) }\n' > "$work/sf.swift"
    swiftc -O "$work/sf.swift" -o "$work/swift_foundation" 2>/dev/null && row "Swift + Foundation" "$work/swift_foundation"
fi
if [ "$(uname)" = Darwin ] && command -v clang >/dev/null 2>&1; then
    printf '#import <Foundation/Foundation.h>\nint main(void){ @autoreleasepool { NSString *h = NSHomeDirectory(); if (h.length == 0) puts("x"); } return 0; }\n' > "$work/o.m"
    clang -O2 -fobjc-arc "$work/o.m" -framework Foundation -o "$work/objc_foundation" && row "ObjC + Foundation" "$work/objc_foundation"
fi
row "bmk -V" "$bmk -V"
row "bmk lookup" "$bmk personal kotlin-app"
row "bmk complete" "$bmk --cli __complete '' personal"

echo "==> hyperfine: $runs runs per command (this takes a minute or two)"
python3 - "$cmds" "$runs" "$work/results.json" "$work/home" <<'EOF'
import os, subprocess, sys
cmds_file, runs, out, home = sys.argv[1:5]
args = ["hyperfine", "-N", "--warmup", "100", "--runs", runs, "--export-json", out, "--style", "none"]
for line in open(cmds_file):
    name, cmd = line.rstrip("\n").split("\t", 1)
    args += ["-n", name, cmd]
env = dict(os.environ, HOME=home)
subprocess.run(args, check=True, env=env, stdout=subprocess.DEVNULL)
EOF

python3 - "$work/results.json" <<'EOF'
import json, sys
results = json.load(open(sys.argv[1]))["results"]
def median_ms(r):
    if "summary" in r:                      # hyperfine 2.x
        return r["summary"]["time_wall_clock"]["median"] * 1000
    return r["median"] * 1000               # hyperfine 1.x
name = lambda r: r.get("name") or r["command"]
med = {name(r): median_ms(r) for r in results}
base = med.get("empty Rust")
print()
print("| Command | Median | vs `true` | vs empty Rust |")
print("|---|---:|---:|---:|")
for n, m in med.items():
    vs_rust = f"{m - base:+.3f} ms" if base is not None else "n/a"
    print(f"| {n} | {m:.3f} ms | {m - med['true']:+.3f} ms | {vs_rust} |")
EOF

if command -v fish >/dev/null 2>&1; then
    echo
    echo "==> In-shell comparison (300 calls each, inside one fish process)"
    HOME="$work/home" BMK="$bmk" fish --no-config -c '
        set -g __bmk (string split \n -- (cat ~/.config/bmk/bookmarks))
        set -g -- __bmkk (string split -f1 \t -- $__bmk)
        set -g -- __bmkv (string replace -r "^~" $HOME -- (string split -f2 \t -- $__bmk))
        function scan_lookup
            set -l key (string join / $argv)
            for l in $__bmk
                set -l f (string split -m2 \t -- $l)
                if test "$f[1]" = $key
                    string replace -r "^~" $HOME -- $f[2]
                    return 0
                end
            end
            return 1
        end
        function preloaded_lookup
            set -l i (contains -i -- (string join / $argv) $__bmkk); and echo $__bmkv[$i]
        end
        function now; python3 -c "import time; print(time.perf_counter_ns())"; end
        printf "| How | Per call |\n|---|---:|\n"
        for how in binary scan_lookup preloaded_lookup
            set -l s (now)
            for i in (seq 300)
                if test $how = binary
                    set -l p ($BMK personal kotlin-app)
                else
                    set -l p ($how personal kotlin-app)
                end
            end
            set -l e (now)
            printf "| %s | %.3f ms |\n" $how (math "($e - $s) / 300 / 1000000")
        end'
fi

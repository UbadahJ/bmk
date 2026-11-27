#!/bin/sh
# bmk installer: the binary, fish completions, config file.
# Usage: ./install.sh [--uninstall]
#
# From a source checkout it builds natively with cargo. From a release archive (a
# prebuilt ./bmk and no Cargo.toml) it copies the binary to $BMK_BIN_DIR, by default
# ~/.local/bin.
set -eu

cd "$(dirname "$0")"

case ${HOME:-} in /*) ;; *) printf 'error: HOME must be set to an absolute path\n' >&2; exit 1 ;; esac

config_home=${XDG_CONFIG_HOME:-}
case $config_home in /*) ;; *) config_home="$HOME/.config" ;; esac

completion_file="$config_home/fish/completions/bmk.fish"
bookmarks_file="$HOME/.config/bmk/bookmarks"
# cargo install puts binaries in $CARGO_INSTALL_ROOT/bin, else $CARGO_HOME/bin.
cargo_bin="${CARGO_INSTALL_ROOT:-${CARGO_HOME:-$HOME/.cargo}}/bin"
prebuilt_bin="${BMK_BIN_DIR:-$HOME/.local/bin}"

info() { printf '==> %s\n' "$*"; }
warn() { printf 'warning: %s\n' "$*" >&2; }
die()  { printf 'error: %s\n' "$*" >&2; exit 1; }

if [ "${1:-}" = "--uninstall" ]; then
    [ $# -eq 1 ] || die "unexpected argument: $2 (usage: ./install.sh --uninstall)"
    command -v cargo >/dev/null 2>&1 && cargo uninstall bmk 2>/dev/null || true
    rm -f "$prebuilt_bin/bmk"
    rm -f "$completion_file"
    info "Uninstalled bmk. Bookmarks kept at $bookmarks_file"
    exit 0
fi

[ $# -eq 0 ] || die "unknown argument: $1 (usage: ./install.sh [--uninstall])"

if [ ! -f Cargo.toml ] && [ -f bmk ]; then
    # 1. Release archive: install the prebuilt (generic) binary. Copy, then rename, so a
    #    running bmk is never overwritten in place.
    info "Installing the prebuilt binary to $prebuilt_bin"
    mkdir -p "$prebuilt_bin"
    cp bmk "$prebuilt_bin/.bmk.tmp.$$"
    chmod 755 "$prebuilt_bin/.bmk.tmp.$$"
    mv -f "$prebuilt_bin/.bmk.tmp.$$" "$prebuilt_bin/bmk"
    bin_dir=$prebuilt_bin
else
    command -v cargo >/dev/null 2>&1 \
        || die "cargo not found. Install Rust from https://rustup.rs, then re-run."

    # 1. Native build. cargo install doesn't reliably pick up .cargo/config.toml,
    #    and an existing RUSTFLAGS would override it, so we pass the flag explicitly.
    #    CARGO_ENCODED_RUSTFLAGS takes precedence over RUSTFLAGS, so extend that one if set.
    info "Building bmk for this CPU (target-cpu=native)"
    if [ -n "${CARGO_ENCODED_RUSTFLAGS:-}" ]; then
        sep=$(printf '\037')
        CARGO_ENCODED_RUSTFLAGS="$CARGO_ENCODED_RUSTFLAGS$sep-C${sep}target-cpu=native" \
            cargo install --path . --locked --force
    else
        RUSTFLAGS="${RUSTFLAGS:+$RUSTFLAGS }-C target-cpu=native" \
            cargo install --path . --locked --force
    fi
    bin_dir=$cargo_bin
fi

bmk="$bin_dir/bmk"
[ -x "$bmk" ] || die "$bmk is missing after the install"

# 2. Fish completions: a stub that sources the script embedded in the binary,
#    so the completions always match the installed version.
if command -v fish >/dev/null 2>&1; then
    info "Installing fish completions: $completion_file"
    mkdir -p "$(dirname "$completion_file")"
    printf '%s\n' 'command -q bmk; and command bmk --cli fish | source' > "$completion_file"
else
    warn "fish not found; skipped completions"
fi

# 3. Config file (never overwritten).
if [ ! -e "$bookmarks_file" ]; then
    info "Creating $bookmarks_file"
    mkdir -p "$(dirname "$bookmarks_file")"
    printf '# key<TAB>path<TAB>tags (optional)\n' > "$bookmarks_file"
fi

# 4. PATH check and smoke test.
case ":$PATH:" in
    *":$bin_dir:"*) ;;
    *) warn "$bin_dir is not on PATH. In fish: fish_add_path $bin_dir" ;;
esac

info "Installed: $("$bmk" --version)"

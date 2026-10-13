#!/usr/bin/env bash
# run installer: builds `run` from source and installs it for the current user.
#
#   curl -fsSL https://raw.githubusercontent.com/uxeric/run/HEAD/install.sh | bash
#   ./install.sh                 # from a checkout: builds that checkout
#   ./install.sh --force         # rebuild even when the installed run is up to date
#   ./install.sh --uninstall     # removes run, the downloaded source, and the install record
#
# Settings (environment variables):
#   RUN_REPO        git URL to clone          (default: https://github.com/uxeric/run.git)
#   RUN_REF         branch or tag to build    (default: the repo's default branch)
#   RUN_PREFIX      install prefix            (default: ~/.local, so run goes in ~/.local/bin)
#   RUN_SOURCE_DIR  build this directory instead of cloning
#   RUN_YES=1       answer yes to prompts (e.g. installing Rust)
#
# The installed revision is recorded so `run` can offer updates.

set -euo pipefail

REPO="${RUN_REPO:-https://github.com/uxeric/run.git}"
REF="${RUN_REF:-}"
PREFIX="${RUN_PREFIX:-$HOME/.local}"
BIN_DIR="$PREFIX/bin"
DATA_DIR="${XDG_DATA_HOME:-$HOME/.local/share}/run"
STAMP="$DATA_DIR/installed-rev"
MIN_RUST="1.88"
FORCE=0

if [[ -t 1 && -z "${NO_COLOR:-}" ]]; then
    ICE=$'\e[38;2;41;240;255m' HOT=$'\e[38;2;255;42;109m' AMBER=$'\e[38;2;246;196;69m'
    LEAF=$'\e[38;2;61;255;154m' GHOST=$'\e[38;2;110;106;130m' BOLD=$'\e[1m' RESET=$'\e[0m'
else
    ICE="" HOT="" AMBER="" LEAF="" GHOST="" BOLD="" RESET=""
fi

step() { printf '%s›%s %s\n' "$HOT" "$RESET" "$*"; }
ok() { printf '  %s✓%s %s\n' "$LEAF" "$RESET" "$*"; }
warn() { printf '  %s!%s %s\n' "$AMBER" "$RESET" "$*" >&2; }
die() { printf '%s✗ %s%s\n' "$HOT" "$*" "$RESET" >&2; exit 1; }

banner() {
    printf '\n%s%s' "$HOT" "$BOLD"
    printf '  ██████    ██    ██   ███    ██\n'
    printf '  ██   ██   ██    ██   ████   ██\n'
    printf '%s  ██████    ██    ██   ██ ██  ██\n' "$AMBER"
    printf '  ██   ██   ██    ██   ██  ██ ██\n'
    printf '%s  ██   ██   ██    ██   ██   ████\n' "$ICE"
    printf '  ██   ██    ██████    ██    ███%s\n\n' "$RESET"
    printf '  %spaste the commands. they run in order · installer%s\n\n' "$GHOST" "$RESET"
}

confirm() {
    [[ "${RUN_YES:-}" == 1 ]] && return 0
    local answer=""
    { exec 3< /dev/tty; } 2> /dev/null || return 1
    printf '  %s?%s %s [y/N] ' "$AMBER" "$RESET" "$1" 2> /dev/null > /dev/tty
    read -r answer <&3 || true
    exec 3<&-
    [[ "$answer" == [yY]* ]]
}

is_omarchy() {
    command -v omarchy-install-dev-env > /dev/null || command -v omarchy-version > /dev/null
}

version_at_least() {
    [[ "$(printf '%s\n%s\n' "$2" "$1" | sort -V | head -n1)" == "$2" ]]
}

ensure_rust() {
    if ! command -v cargo > /dev/null && [[ -x "$HOME/.cargo/bin/cargo" ]]; then
        export PATH="$HOME/.cargo/bin:$PATH"
    fi
    if command -v cargo > /dev/null; then
        local have
        have="$(rustc --version | awk '{print $2}')"
        version_at_least "$have" "$MIN_RUST" || die "run needs Rust $MIN_RUST or newer (found $have). Run: rustup update"
        ok "Rust $have"
        return
    fi
    warn "Rust is not installed."
    if is_omarchy && command -v omarchy-install-dev-env > /dev/null; then
        confirm "Install Rust the Omarchy way (omarchy-install-dev-env rust)?" \
            || die "Install Rust ($MIN_RUST+) and run this again."
        omarchy-install-dev-env rust
        export PATH="$HOME/.cargo/bin:$PATH"
    elif command -v pacman > /dev/null; then
        confirm "Install Rust with pacman (sudo pacman -S --needed rust)?" \
            || die "Install Rust ($MIN_RUST+) and run this again."
        sudo pacman -S --needed --noconfirm rust
    else
        confirm "Install Rust with rustup (https://rustup.rs)?" \
            || die "Install Rust ($MIN_RUST+) and run this again."
        curl --proto '=https' --tlsv1.2 -fsSL https://sh.rustup.rs | sh -s -- -y --profile minimal
        export PATH="$HOME/.cargo/bin:$PATH"
    fi
    command -v cargo > /dev/null || die "Rust was installed but cargo is not on PATH. Open a new shell and run this again."
    local have
    have="$(rustc --version | awk '{print $2}')"
    version_at_least "$have" "$MIN_RUST" || die "run needs Rust $MIN_RUST or newer (found $have). Run: rustup update"
    ok "Rust $have"
}

is_checkout() {
    [[ -f "$1/Cargo.toml" && -f "$1/install.sh" ]] && grep -q '^name = "run"' "$1/Cargo.toml"
}

find_source() {
    if [[ -n "${RUN_SOURCE_DIR:-}" ]]; then
        is_checkout "$RUN_SOURCE_DIR" || die "RUN_SOURCE_DIR=$RUN_SOURCE_DIR is not a run checkout."
        SOURCE="$RUN_SOURCE_DIR"
        ok "building $SOURCE"
        return
    fi
    local here=""
    if [[ -n "${BASH_SOURCE[0]:-}" && -f "${BASH_SOURCE[0]}" ]]; then
        here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
    fi
    if [[ -n "$here" ]] && is_checkout "$here"; then
        SOURCE="$here"
        ok "building this checkout ($SOURCE)"
        return
    fi
    command -v git > /dev/null || die "git is needed to download run. Install git and run this again."
    SOURCE="$DATA_DIR/src"
    if [[ -d "$SOURCE/.git" ]]; then
        git -C "$SOURCE" fetch --quiet --depth 1 origin "${REF:-HEAD}" || die "Could not update $SOURCE from $REPO."
        git -C "$SOURCE" checkout --quiet --force FETCH_HEAD
        ok "updated $SOURCE (${REF:-default branch})"
    else
        mkdir -p "$DATA_DIR"
        local branch=()
        [[ -n "$REF" ]] && branch=(--branch "$REF")
        git clone --quiet --depth 1 "${branch[@]}" "$REPO" "$SOURCE" \
            || die "Could not clone $REPO${REF:+ ($REF)}. Set RUN_REPO (or RUN_REF) to the right value."
        ok "downloaded $REPO (${REF:-default branch})"
    fi
}

source_rev() {
    git -C "$SOURCE" rev-parse --is-inside-work-tree > /dev/null 2>&1 || return 0
    [[ -z "$(git -C "$SOURCE" status --porcelain --untracked-files=no 2> /dev/null)" ]] || return 0
    git -C "$SOURCE" rev-parse --short HEAD 2> /dev/null || true
}

check_path() {
    case ":$PATH:" in
        *":$BIN_DIR:"*) ;;
        *)
            warn "$BIN_DIR is not on your PATH. Add this to your shell's rc file:"
            printf '      export PATH="%s:$PATH"\n' "$BIN_DIR" >&2
            ;;
    esac
}

finish() {
    check_path
    printf '\n  %s%s%s%s Paste into %s%srun%s. It runs when the paste ends.\n\n' \
        "$ICE" "$BOLD" "$1" "$RESET" "$HOT" "$BOLD" "$RESET"
}

install_run() {
    banner
    step "checking the toolchain"
    ensure_rust
    step "getting the source"
    find_source
    local rev old=""
    rev="$(source_rev)"
    if [[ -x "$BIN_DIR/run" ]]; then
        [[ -f "$STAMP" ]] && old="$(< "$STAMP")"
        if [[ -n "$rev" && "$rev" == "$old" && "$FORCE" != 1 ]]; then
            ok "run $rev is already installed and up to date (--force rebuilds it anyway)"
            finish "already ready."
            return
        fi
        step "updating run ${old:-(earlier build)} → ${rev:-local build}"
    else
        step "installing run ${rev:-(local build)}"
    fi
    step "compiling (this takes a minute the first time)"
    (cd "$SOURCE" && cargo build --release --locked --quiet) || die "The build failed. The compiler output above says why."
    ok "built target/release/run"
    step "installing"
    install -Dm755 "$SOURCE/target/release/run" "$BIN_DIR/run"
    ok "installed $BIN_DIR/run"
    mkdir -p "$DATA_DIR"
    if [[ -n "$rev" ]]; then
        printf '%s\n' "$rev" > "$STAMP"
        ok "recorded $rev for update checks"
    else
        rm -f "$STAMP"
    fi
    finish "ready."
}

uninstall_run() {
    step "uninstalling"
    if [[ -e "$BIN_DIR/run" ]]; then
        rm -f "$BIN_DIR/run"
        ok "removed $BIN_DIR/run"
    else
        ok "run was not installed in $BIN_DIR"
    fi
    if [[ -d "$DATA_DIR" ]]; then
        rm -rf "$DATA_DIR"
        ok "removed $DATA_DIR (downloaded source, install record)"
    fi
}

main() {
    local mode=install
    for arg in "$@"; do
        case "$arg" in
            --uninstall) mode=uninstall ;;
            --force) FORCE=1 ;;
            -h | --help)
                sed -n '2,16p' "${BASH_SOURCE[0]:-/dev/null}" 2> /dev/null | sed 's/^# \{0,1\}//'
                exit 0
                ;;
            *) die "unknown option: $arg (try --help)" ;;
        esac
    done
    if [[ "$mode" == uninstall ]]; then
        uninstall_run
    else
        install_run
    fi
}

if [[ -n ${BASH_SOURCE[0]:-} && ${BASH_SOURCE[0]} != "$0" ]]; then
    :
else
    main "$@"
fi

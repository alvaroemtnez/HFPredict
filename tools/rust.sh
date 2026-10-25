#!/usr/bin/env bash
# rust.sh - Run cargo (or any Rust tool) with the toolchain pinned in
# rust-toolchain.toml, installed into .tools/ on first use.
#
# Usage: tools/rust.sh cargo build ...      run a tool with the pinned toolchain
#        tools/rust.sh env                  print exports: eval "$(tools/rust.sh env)"
#
# Nothing is installed system-wide: rustup keeps the toolchain in
# .tools/rustup and cargo its registry in .tools/cargo. The same directory
# is mounted into the podman containers, so they use this toolchain too.

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
export RUSTUP_HOME="$ROOT/.tools/rustup"
export CARGO_HOME="$ROOT/.tools/cargo"
export PATH="$CARGO_HOME/bin:$PATH"

setup() {
    [ -x "$CARGO_HOME/bin/rustup" ] && return
    mkdir -p "$RUSTUP_HOME" "$CARGO_HOME"
    echo "==> Installing rustup into .tools/ (no system changes)" >&2
    # rustup-init verifies the toolchain it downloads; the script itself
    # comes over TLS from the Rust project.
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs |
        sh -s -- -y --no-modify-path --profile minimal --default-toolchain none >&2
}

setup
cd "$ROOT"
case "${1:-}" in
    env)
        printf 'export RUSTUP_HOME=%q CARGO_HOME=%q\n' "$RUSTUP_HOME" "$CARGO_HOME"
        printf 'export PATH=%q:"$PATH"\n' "$CARGO_HOME/bin"
        ;;
    "") echo "usage: tools/rust.sh <command> [args...]" >&2; exit 2 ;;
    *) exec "$@" ;;
esac

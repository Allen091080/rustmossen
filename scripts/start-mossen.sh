#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

# Keep interactive launches clean. Development checks should still use the
# normal cargo commands without this wrapper when warning output is desired.
export RUSTFLAGS="${RUSTFLAGS:-} -Awarnings"

BIN="$ROOT/target/debug/mossen"
BUILD_MODE="${MOSSEN_START_BUILD:-auto}"

needs_build=0
case "$BUILD_MODE" in
  always)
    needs_build=1
    ;;
  never)
    if [[ ! -x "$BIN" ]]; then
      echo "mossen binary is missing: $BIN" >&2
      echo "Run without MOSSEN_START_BUILD=never or build it with: cargo build -p mossen-cli --bin mossen" >&2
      exit 1
    fi
    ;;
  auto)
    if [[ ! -x "$BIN" ]]; then
      needs_build=1
    elif find "$ROOT/Cargo.toml" "$ROOT/Cargo.lock" "$ROOT/crates" \
      -type f \( -name '*.rs' -o -name 'Cargo.toml' -o -name 'Cargo.lock' \) \
      -newer "$BIN" -print -quit | grep -q .; then
      needs_build=1
    fi
    ;;
  *)
    echo "Invalid MOSSEN_START_BUILD=$BUILD_MODE (expected auto, always, or never)" >&2
    exit 1
    ;;
esac

if [[ "$needs_build" == "1" ]]; then
  cargo build --quiet -p mossen-cli --bin mossen
fi

exec "$BIN" "$@"

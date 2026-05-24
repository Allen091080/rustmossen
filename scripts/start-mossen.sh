#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

# Keep interactive launches clean. Development checks should still use the
# normal cargo commands without this wrapper when warning output is desired.
export RUSTFLAGS="${RUSTFLAGS:-} -Awarnings"

cargo build --quiet -p mossen-cli --bin mossen
exec "$ROOT/target/debug/mossen" "$@"

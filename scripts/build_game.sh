#!/usr/bin/env bash
#
# Build game/ to assets/game.wasm and record its hash.
#
#   scripts/build_game.sh          build, copy, rewrite the hash file
#   scripts/build_game.sh --check  verify the committed binary against the hash
#
# The --check mode does not rebuild. It cannot: reproducing a byte-identical
# binary needs the same rustc version, and pinning that would mean shipping a
# toolchain file and a CI Rust install for a page that is otherwise built by
# nothing at all. So be plain about what the hash proves. It proves the
# committed .wasm is the one that was reviewed and has not been altered since.
# It does not prove the .wasm is what game/src compiles to. Anyone who wants
# that runs this script without --check and looks at the diff, which is the
# same trust model as scripts/build-corpus.html and no better.

set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
wasm="$here/assets/game.wasm"
hashfile="$here/assets/game.wasm.sha256"

sha() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | cut -d' ' -f1
  else
    shasum -a 256 "$1" | cut -d' ' -f1
  fi
}

if [ "${1:-}" = "--check" ]; then
  if [ ! -f "$wasm" ] || [ ! -f "$hashfile" ]; then
    echo "missing assets/game.wasm or assets/game.wasm.sha256" >&2
    exit 1
  fi
  want="$(cut -d' ' -f1 <"$hashfile")"
  got="$(sha "$wasm")"
  if [ "$want" != "$got" ]; then
    echo "assets/game.wasm does not match its recorded hash." >&2
    echo "  recorded $want" >&2
    echo "  actual   $got" >&2
    echo "Rebuild with scripts/build_game.sh and commit both files." >&2
    exit 1
  fi
  echo "assets/game.wasm matches $want"
  exit 0
fi

# Which rustc and which cargo.
#
# On a Mac with Homebrew's `rust` formula installed, `cargo` and `rustc` on
# PATH belong to Homebrew, whose standard library has no wasm32 target, while
# the wasm target sits inside rustup. `rustup target add` does not fix that and
# neither does `rustup run`, because cargo still finds Homebrew's rustc first.
# The fix is to name both binaries explicitly. Detecting the situation and
# saying so is better than carrying one machine's absolute path in a script.
if ! command -v rustup >/dev/null 2>&1; then
  echo "rustup is not installed." >&2
  echo "Install it from https://rustup.rs, then: rustup target add wasm32-unknown-unknown" >&2
  exit 1
fi

CARGO="$(rustup which cargo)"
RUSTC="$(rustup which rustc)"
sysroot="$("$RUSTC" --print sysroot)"

if [ ! -d "$sysroot/lib/rustlib/wasm32-unknown-unknown" ]; then
  echo "The wasm32-unknown-unknown standard library is not installed for this toolchain." >&2
  echo "  rustup target add wasm32-unknown-unknown" >&2
  exit 1
fi

if command -v cargo >/dev/null 2>&1; then
  path_cargo="$(command -v cargo)"
  if [ "$path_cargo" != "$CARGO" ]; then
    echo "note: cargo on PATH is $path_cargo, which is not the rustup one."
    echo "      Using $CARGO instead. This is the Homebrew rust shadowing case;"
    echo "      nothing is wrong, but plain 'cargo build --target wasm32-...' will fail."
  fi
fi

echo "building with $RUSTC"
RUSTC="$RUSTC" "$CARGO" build \
  --release \
  --target wasm32-unknown-unknown \
  --manifest-path "$here/game/Cargo.toml"

built="$here/game/target/wasm32-unknown-unknown/release/helm.wasm"
mkdir -p "$here/assets"
cp "$built" "$wasm"

digest="$(sha "$wasm")"
printf '%s  game.wasm\n' "$digest" >"$hashfile"

bytes=$(wc -c <"$wasm" | tr -d ' ')
gz=$(gzip -9 -c "$wasm" | wc -c | tr -d ' ')
echo "assets/game.wasm  $bytes bytes, $gz gzipped"
echo "sha256 $digest"

#!/usr/bin/env bash
# Builds xpando's patched espanso engine for this OS and stages it as the Tauri sidecar:
#   xpando-app/src-tauri/binaries/xpando-engine-<target-triple>[.exe]
# Flags are the ones upstream's .github/workflows/ci.yml uses (see docs/xpando/ENGINE.md).
# Fails unless `--version` ends in `-xpando`. Needs the OS libraries listed in ENGINE.md.
set -euo pipefail
cd "$(dirname "$0")/.."

out=xpando-app/src-tauri/binaries
mkdir -p "$out"
host="$(rustc -vV | sed -n 's/^host: //p')"

verify() { # path
  local v
  v="$("$1" --version)"
  echo "$1: $v"
  case "$v" in
    *-xpando) ;;
    *) echo "engine version does not end in -xpando" >&2; exit 1 ;;
  esac
}

case "$(uname -s)" in
  Darwin)
    features=native-tls,modulo
    for t in aarch64-apple-darwin x86_64-apple-darwin; do
      rustup target add "$t"
      cargo build -p espanso --release --locked --target "$t" --no-default-features --features "$features"
      cp "target/$t/release/espanso" "$out/xpando-engine-$t"
    done
    lipo -create -output "$out/xpando-engine-universal-apple-darwin" \
      "$out/xpando-engine-aarch64-apple-darwin" "$out/xpando-engine-x86_64-apple-darwin"
    verify "$out/xpando-engine-universal-apple-darwin"
    ;;
  Linux)
    # X11 build. For a Wayland build add ",wayland" to the features.
    cargo build -p espanso --release --locked --no-default-features --features modulo,vendored-tls
    cp target/release/espanso "$out/xpando-engine-$host"
    verify "$out/xpando-engine-$host"
    ;;
  *) # Windows (Git Bash): default features
    cargo build -p espanso --release --locked
    cp target/release/espanso.exe "$out/xpando-engine-$host.exe"
    verify "$out/xpando-engine-$host.exe"
    ;;
esac
ls -l "$out"

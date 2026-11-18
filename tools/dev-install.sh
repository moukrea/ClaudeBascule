#!/usr/bin/env bash
# Build Claude Bascule from this checkout and install it like a release (Linux and macOS).
set -euo pipefail
cd "$(dirname "$0")/.."
out=$(mktemp -d)
trap 'rm -rf "$out"' EXIT
case "$(uname -s)" in
  Linux)
    nice cargo build --release -p bascule -p bascule-app
    dist/package.sh linux target/release "$out"
    CLAUDE_BASCULE_ARCHIVE="$out/claude-bascule-linux-x86_64.tar.gz" sh dist/install.sh ;;
  Darwin)
    cargo build --release -p bascule -p bascule-app
    dist/package.sh macos target/release "$out" 0.0.0
    CLAUDE_BASCULE_ARCHIVE="$out/claude-bascule-macos-universal.tar.gz" sh dist/install.sh ;;
  *) echo "On Windows, build with cargo and run dist/install.ps1 with CLAUDE_BASCULE_ARCHIVE." >&2; exit 2 ;;
esac

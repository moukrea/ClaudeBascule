#!/usr/bin/env bash
# Package release binaries into the archive install.sh expects.
#   dist/package.sh linux  <dir with bascule, bascule-app>  <out dir>
#   dist/package.sh macos  <dir with bascule, bascule-app (universal)>  <out dir>  <version>
set -euo pipefail
here=$(cd "$(dirname "$0")/.." && pwd)
kind=$1 bin=$2 out=$3 version=${4:-0.0.0}
mkdir -p "$out"
stage=$(mktemp -d)
trap 'rm -rf "$stage"' EXIT
case "$kind" in
  linux)
    install -m 0755 "$bin/bascule" "$bin/bascule-app" "$stage/"
    install -m 0644 "$here/app/icons/icon.png" "$stage/claude-bascule.png"
    install -m 0644 "$here/assets/icon.svg" "$stage/claude-bascule.svg"
    tar -czf "$out/claude-bascule-linux-x86_64.tar.gz" -C "$stage" .
    ;;
  macos)
    app="$stage/Claude Bascule.app"
    mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
    install -m 0755 "$bin/bascule" "$bin/bascule-app" "$app/Contents/MacOS/"
    cp "$here/app/icons/icon.icns" "$app/Contents/Resources/icon.icns"
    cat > "$app/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>Claude Bascule</string>
  <key>CFBundleDisplayName</key><string>Claude Bascule</string>
  <key>CFBundleIdentifier</key><string>io.github.moukrea.bascule</string>
  <key>CFBundleExecutable</key><string>bascule-app</string>
  <key>CFBundleIconFile</key><string>icon</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>$version</string>
  <key>CFBundleVersion</key><string>$version</string>
  <key>LSMinimumSystemVersion</key><string>11.0</string>
  <key>LSApplicationCategoryType</key><string>public.app-category.developer-tools</string>
  <key>NSHighResolutionCapable</key><true/>
</dict>
</plist>
PLIST
    # Ad-hoc signature: required to run on Apple silicon (no notarisation).
    codesign --force --deep --sign - "$app"
    tar -czf "$out/claude-bascule-macos-universal.tar.gz" -C "$stage" "Claude Bascule.app"
    ;;
  *) echo "unknown kind $kind" >&2; exit 2 ;;
esac

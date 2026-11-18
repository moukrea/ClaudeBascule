#!/bin/sh
# Claude Bascule installer for Linux and macOS. Installs or updates, for the current user only.
#
#   curl -fsSL https://github.com/moukrea/ClaudeBascule/releases/latest/download/install.sh | sh
#
# Options (environment):
#   CLAUDE_BASCULE_VERSION=1.2.3     a given release instead of the latest
#   CLAUDE_BASCULE_ARCHIVE=path      install from a local archive (development)
#   CLAUDE_BASCULE_NO_LAUNCH=1       do not open the app at the end
set -eu

REPO="moukrea/ClaudeBascule"
ID="io.github.moukrea.claudebascule"

say() { printf '\033[1m%s\033[0m\n' "$*"; }
warn() { printf '\033[33m%s\033[0m\n' "$*" >&2; }
die() { printf '\033[31mError: %s\033[0m\n' "$*" >&2; exit 1; }

fetch() { # url dest
  if command -v curl >/dev/null 2>&1; then curl -fsSL --retry 3 -o "$2" "$1"
  elif command -v wget >/dev/null 2>&1; then wget -q -O "$2" "$1"
  else die "curl or wget is needed"; fi
}

sha256() {
  if command -v sha256sum >/dev/null 2>&1; then sha256sum "$1" | cut -d' ' -f1
  else shasum -a 256 "$1" | cut -d' ' -f1; fi
}

os=$(uname -s)
arch=$(uname -m)
case "$os" in
  Linux)
    [ "$arch" = "x86_64" ] || die "Linux $arch is not supported yet (x86_64 only)."
    asset="claude-bascule-linux-x86_64.tar.gz" ;;
  Darwin) asset="claude-bascule-macos-universal.tar.gz" ;;
  *) die "$os is not supported by this script. On Windows, run in PowerShell: irm https://github.com/$REPO/releases/latest/download/install.ps1 | iex" ;;
esac

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT INT TERM

if [ -n "${CLAUDE_BASCULE_ARCHIVE:-}" ]; then
  cp "$CLAUDE_BASCULE_ARCHIVE" "$tmp/$asset"
else
  if [ -n "${CLAUDE_BASCULE_VERSION:-}" ]; then
    base="https://github.com/$REPO/releases/download/v${CLAUDE_BASCULE_VERSION#v}"
  else
    base="https://github.com/$REPO/releases/latest/download"
  fi
  say "Downloading Claude Bascule…"
  fetch "$base/$asset" "$tmp/$asset"
  fetch "$base/SHA256SUMS" "$tmp/SHA256SUMS"
  want=$(grep " $asset\$" "$tmp/SHA256SUMS" | cut -d' ' -f1)
  [ -n "$want" ] && [ "$want" = "$(sha256 "$tmp/$asset")" ] || die "the download is corrupted (checksum mismatch)"
fi
mkdir "$tmp/x"
tar -xzf "$tmp/$asset" -C "$tmp/x"

# Where claude and jaunt live, so the background service finds them.
extra_path=""
for tool in claude jaunt; do
  p=$(command -v "$tool" 2>/dev/null || true)
  if [ -n "$p" ]; then
    d=$(dirname "$p")
    [ "$d" = "$HOME/.local/bin" ] && continue
    case ":$extra_path:" in *":$d:"*) ;; *) extra_path="${extra_path:+$extra_path:}$d" ;; esac
  fi
done
command -v claude >/dev/null 2>&1 || warn "Claude Code (claude) was not found on this machine: install it first, Claude Bascule switches its account."

if [ "$os" = "Linux" ]; then
  bin="$HOME/.local/bin"
  data="${XDG_DATA_HOME:-$HOME/.local/share}"
  conf="${XDG_CONFIG_HOME:-$HOME/.config}"
  mkdir -p "$bin" "$data/applications" "$data/icons/hicolor/256x256/apps"
  # Replace the binaries without disturbing running copies (new file, then rename).
  for b in bascule bascule-app; do
    cp "$tmp/x/$b" "$bin/.$b.new"
    chmod 0755 "$bin/.$b.new"
    mv -f "$bin/.$b.new" "$bin/$b"
  done
  cp "$tmp/x/claude-bascule.png" "$data/icons/hicolor/256x256/apps/claude-bascule.png"
  cat > "$data/applications/claude-bascule.desktop" <<EOF
[Desktop Entry]
Type=Application
Name=Claude Bascule
GenericName=Claude account switcher
Comment=Switch Claude Code between your accounts before a usage limit
Exec=$bin/bascule-app
Icon=claude-bascule
Terminal=false
Categories=Utility;Development;
StartupWMClass=bascule-app
EOF
  # Earlier development installs used other names.
  rm -f "$data/applications/bascule.desktop" "$data/icons/hicolor/256x256/apps/bascule.png"
  update-desktop-database "$data/applications" >/dev/null 2>&1 || true
  gtk-update-icon-cache -q "$data/icons/hicolor" >/dev/null 2>&1 || true

  svc_path="$bin${extra_path:+:$extra_path}:/usr/local/bin:/usr/bin:/bin"
  if command -v systemctl >/dev/null 2>&1 && systemctl --user show-environment >/dev/null 2>&1; then
    mkdir -p "$conf/systemd/user"
    for old in bascule.service bascule-observer.service; do
      if [ -f "$conf/systemd/user/$old" ]; then
        systemctl --user disable --now "$old" >/dev/null 2>&1 || true
        rm -f "$conf/systemd/user/$old"
      fi
    done
    cat > "$conf/systemd/user/claude-bascule.service" <<EOF
[Unit]
Description=Claude Bascule (switches Claude Code between your accounts)
Documentation=https://github.com/$REPO

[Service]
ExecStart=$bin/bascule daemon
Restart=on-failure
RestartSec=10
Environment=PATH=$svc_path
Environment=DISABLE_AUTOUPDATER=1
Nice=10

[Install]
WantedBy=default.target
EOF
    systemctl --user daemon-reload
    systemctl --user enable claude-bascule.service >/dev/null 2>&1
    systemctl --user restart claude-bascule.service
  else
    # No systemd user session: start with the desktop session instead.
    mkdir -p "$conf/autostart"
    cat > "$conf/autostart/claude-bascule-service.desktop" <<EOF
[Desktop Entry]
Type=Application
Name=Claude Bascule service
Exec=env PATH=$svc_path DISABLE_AUTOUPDATER=1 $bin/bascule daemon
NoDisplay=true
X-GNOME-Autostart-enabled=true
EOF
    pkill -f "$bin/bascule daemon" 2>/dev/null || true
    (PATH="$svc_path" DISABLE_AUTOUPDATER=1 nohup "$bin/bascule" daemon >/dev/null 2>&1 &)
  fi

  # The window needs WebKitGTK 4.1.
  if command -v ldconfig >/dev/null 2>&1 && ! ldconfig -p 2>/dev/null | grep -q 'libwebkit2gtk-4.1'; then
    warn "The window needs WebKitGTK 4.1. Install it with your package manager, for example:"
    warn "  Debian/Ubuntu: sudo apt install libwebkit2gtk-4.1-0    Fedora: sudo dnf install webkit2gtk4.1    Arch: sudo pacman -S webkit2gtk-4.1"
  fi

  case ":$PATH:" in *":$bin:"*) ;; *) warn "Add $bin to your PATH to use the 'bascule' command." ;; esac

  if [ -z "${CLAUDE_BASCULE_NO_LAUNCH:-}" ] && { [ -n "${DISPLAY:-}" ] || [ -n "${WAYLAND_DISPLAY:-}" ]; }; then
    pkill -x bascule-app 2>/dev/null || true
    sleep 1
    if command -v systemd-run >/dev/null 2>&1; then
      systemd-run --user --collect --quiet --unit "claude-bascule-app-$$" "$bin/bascule-app" >/dev/null 2>&1 \
        || (nohup "$bin/bascule-app" >/dev/null 2>&1 &)
    else
      (nohup "$bin/bascule-app" >/dev/null 2>&1 &)
    fi
  fi
else
  apps="$HOME/Applications"
  app="$apps/Claude Bascule.app"
  mkdir -p "$apps"
  pkill -x bascule-app 2>/dev/null || true
  rm -rf "$app.new"
  cp -R "$tmp/x/Claude Bascule.app" "$app.new"
  xattr -dr com.apple.quarantine "$app.new" 2>/dev/null || true
  rm -rf "$app"
  mv "$app.new" "$app"
  # The command line tool.
  mkdir -p "$HOME/.local/bin"
  ln -sf "$app/Contents/MacOS/bascule" "$HOME/.local/bin/bascule"
  # The background service, started at login.
  agents="$HOME/Library/LaunchAgents"
  plist="$agents/$ID.plist"
  mkdir -p "$agents" "$HOME/Library/Logs"
  svc_path="$HOME/.local/bin${extra_path:+:$extra_path}:/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin"
  cat > "$plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key><string>$ID</string>
  <key>ProgramArguments</key>
  <array><string>$app/Contents/MacOS/bascule</string><string>daemon</string></array>
  <key>EnvironmentVariables</key>
  <dict>
    <key>PATH</key><string>$svc_path</string>
    <key>DISABLE_AUTOUPDATER</key><string>1</string>
  </dict>
  <key>RunAtLoad</key><true/>
  <key>KeepAlive</key><dict><key>SuccessfulExit</key><false/></dict>
  <key>ProcessType</key><string>Background</string>
  <key>StandardErrorPath</key><string>$HOME/Library/Logs/ClaudeBascule.log</string>
</dict>
</plist>
EOF
  uid=$(id -u)
  launchctl bootout "gui/$uid/$ID" >/dev/null 2>&1 || true
  launchctl bootstrap "gui/$uid" "$plist"
  [ -n "${CLAUDE_BASCULE_NO_LAUNCH:-}" ] || open "$app"
fi

say "Claude Bascule is installed."

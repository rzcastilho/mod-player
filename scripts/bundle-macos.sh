#!/usr/bin/env bash
# Wraps the built `modplayer` binary in a minimal macOS `.app` bundle at
# `target/<profile>/ModPlayer.app` so it can be launched through Launch
# Services (`open`). A bare binary started from a terminal — in particular
# from a shell inside tmux — is drawn by the window server but never
# registers as a foreground application, so it cannot become active and
# keyboard input keeps going to the terminal. Launching the bundle fixes
# that; see README.md "Quickstart".
#
# Usage: scripts/bundle-macos.sh [release|debug]   (default: release)
set -euo pipefail

profile="${1:-release}"
root="$(cd "$(dirname "$0")/.." && pwd)"
binary="$root/target/$profile/modplayer"
app="$root/target/$profile/ModPlayer.app"

if [[ "$(uname -s)" != "Darwin" ]]; then
    echo "bundle-macos.sh: macOS only" >&2
    exit 1
fi
if [[ ! -x "$binary" ]]; then
    echo "bundle-macos.sh: $binary not found — run 'cargo build --$profile -p modplayer' first" >&2
    exit 1
fi

version="$(sed -n 's/^version = "\(.*\)"/\1/p' "$root/crates/modplayer/Cargo.toml" | head -n 1)"

rm -rf "$app"
mkdir -p "$app/Contents/MacOS"
cp "$binary" "$app/Contents/MacOS/modplayer"
cat > "$app/Contents/Info.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>ModPlayer</string>
  <key>CFBundleDisplayName</key><string>ModPlayer</string>
  <key>CFBundleIdentifier</key><string>dev.rzcastilho.modplayer</string>
  <key>CFBundleExecutable</key><string>modplayer</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleVersion</key><string>${version}</string>
  <key>CFBundleShortVersionString</key><string>${version}</string>
  <key>NSHighResolutionCapable</key><true/>
  <key>LSMinimumSystemVersion</key><string>12.0</string>
</dict>
</plist>
EOF

echo "$app"

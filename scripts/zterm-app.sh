#!/bin/sh
# Build the terminal example as zTerm.app (the dist profile), its interface
# compiled in (terminal-bundle), and install it: ./scripts/zterm-app.sh [destination]
# (default /Applications). Needs Zig 0.16 for Ghostty (see
# crates/zgx-ghostty-sys). ZTERM_DEFAULTS=1 also makes it what opens shell
# scripts and ssh:// links (scripts/zterm-defaults.swift).
set -eu
root=$(cd "$(dirname "$0")/.." && pwd)
dest=${1:-/Applications}
cd "$root"
cargo build --profile dist -p zgx-desktop --features ghostty,terminal-bundle --example terminal

app="$dest/zTerm.app"
staging=$(mktemp -d)/zTerm.app
mkdir -p "$staging/Contents/MacOS" "$staging/Contents/Resources"
cp target/dist/examples/terminal "$staging/Contents/MacOS/zterm"
if [ -f scripts/zterm.icns ]; then
  cp scripts/zterm.icns "$staging/Contents/Resources/zterm.icns"
fi

# Ghostty's resources, where it looks for them beside its executable: its
# terminfo (TERM=xterm-ghostty, rather than xterm-256color without it) and
# its shell integration (the shell's directory for new panes, prompt marks,
# titles). From the Ghostty source the build fetched (crates/zgx-ghostty-sys).
commit=$(sed -n 's/^const COMMIT: &str = "\([0-9a-f]*\)";/\1/p' crates/zgx-ghostty-sys/build.rs | cut -c1-12)
cache=${ZGX_GHOSTTY_CACHE:-$HOME/Library/Caches/zgx-ghostty}
ghostty=${GHOSTTY_SOURCE_DIR:-$cache/ghostty-$commit}
zig=${ZIG:-$( [ -x /opt/homebrew/opt/zig@0.16/bin/zig ] && echo /opt/homebrew/opt/zig@0.16/bin/zig || echo zig )}
resources="$staging/Contents/Resources"
mkdir -p "$resources/ghostty"
cp -R "$ghostty/src/shell-integration" "$resources/ghostty/shell-integration"
find "$resources/ghostty/shell-integration" -name '*.md' -delete
work=$(mktemp -d)
cp -R "$ghostty/src/terminfo" "$work/terminfo"
cat > "$work/main.zig" <<'ZIG'
const std = @import("std");
pub fn main(init: std.process.Init) !void {
    var buffer: [1024]u8 = undefined;
    var stdout = std.Io.File.stdout().writerStreaming(init.io, &buffer);
    try @import("terminfo/ghostty.zig").ghostty.encode(&stdout.interface);
    try stdout.end();
}
ZIG
(cd "$work" && "$zig" run main.zig > ghostty.terminfo)
tic -x -o "$resources/terminfo" "$work/ghostty.terminfo" 2>/dev/null
rm -rf "$work"
# The integration's `ssh` runs `ghostty +ssh` from beside the executable.
cp scripts/zterm-ghostty.sh "$staging/Contents/MacOS/ghostty"
chmod +x "$staging/Contents/MacOS/ghostty"

cat > "$staging/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>zTerm</string>
  <key>CFBundleDisplayName</key><string>zTerm</string>
  <key>CFBundleIdentifier</key><string>sh.zeron.zterm</string>
  <key>CFBundleExecutable</key><string>zterm</string>
  <key>CFBundleIconFile</key><string>zterm</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>0.1.0</string>
  <key>CFBundleVersion</key><string>1</string>
  <key>LSMinimumSystemVersion</key><string>13.0</string>
  <key>LSApplicationCategoryType</key><string>public.app-category.developer-tools</string>
  <key>NSHighResolutionCapable</key><true/>
  <key>NSSupportsAutomaticGraphicsSwitching</key><true/>
  <!-- What it opens (crates/zgx-desktop/examples/terminal/opened.rs). -->
  <key>CFBundleDocumentTypes</key>
  <array>
    <dict>
      <key>CFBundleTypeName</key><string>Folder</string>
      <key>CFBundleTypeRole</key><string>Viewer</string>
      <key>LSHandlerRank</key><string>Alternate</string>
      <key>LSItemContentTypes</key><array><string>public.folder</string></array>
    </dict>
    <dict>
      <key>CFBundleTypeName</key><string>Shell Script</string>
      <key>CFBundleTypeRole</key><string>Shell</string>
      <key>LSHandlerRank</key><string>Alternate</string>
      <key>LSItemContentTypes</key>
      <array>
        <string>com.apple.terminal.shell-script</string>
        <string>public.shell-script</string>
        <string>public.unix-executable</string>
      </array>
    </dict>
  </array>
  <key>CFBundleURLTypes</key>
  <array>
    <dict>
      <key>CFBundleURLName</key><string>SSH</string>
      <key>CFBundleURLSchemes</key><array><string>ssh</string></array>
    </dict>
    <dict>
      <key>CFBundleURLName</key><string>Manual Page</string>
      <key>CFBundleURLSchemes</key><array><string>x-man-page</string></array>
    </dict>
  </array>
  <!-- Its shells' programs ask for these as zTerm: macOS needs a reason to
       ask the user, else it refuses them outright. -->
  <key>NSAppleEventsUsageDescription</key><string>A program running in zTerm wants to control another app.</string>
  <key>NSDesktopFolderUsageDescription</key><string>A program running in zTerm wants to access your Desktop folder.</string>
  <key>NSDocumentsFolderUsageDescription</key><string>A program running in zTerm wants to access your Documents folder.</string>
  <key>NSDownloadsFolderUsageDescription</key><string>A program running in zTerm wants to access your Downloads folder.</string>
  <key>NSRemovableVolumesUsageDescription</key><string>A program running in zTerm wants to access files on a removable volume.</string>
  <key>NSNetworkVolumesUsageDescription</key><string>A program running in zTerm wants to access files on a network volume.</string>
  <key>NSSystemAdministrationUsageDescription</key><string>A program running in zTerm wants to change system settings.</string>
  <key>NSLocalNetworkUsageDescription</key><string>A program running in zTerm wants to connect to devices on your local network.</string>
  <key>NSCameraUsageDescription</key><string>A program running in zTerm wants to use the camera.</string>
  <key>NSMicrophoneUsageDescription</key><string>A program running in zTerm wants to use the microphone.</string>
  <key>NSBluetoothAlwaysUsageDescription</key><string>A program running in zTerm wants to use Bluetooth.</string>
  <key>NSContactsUsageDescription</key><string>A program running in zTerm wants to access your contacts.</string>
  <key>NSCalendarsUsageDescription</key><string>A program running in zTerm wants to access your calendars.</string>
  <key>NSRemindersUsageDescription</key><string>A program running in zTerm wants to access your reminders.</string>
  <key>NSPhotoLibraryUsageDescription</key><string>A program running in zTerm wants to access your photos.</string>
  <key>NSLocationUsageDescription</key><string>A program running in zTerm wants to use your location.</string>
</dict>
</plist>
PLIST
plutil -lint "$staging/Contents/Info.plist" >/dev/null
# Signed with a stable identity, macOS keeps what the user allowed it (its
# shells reading ~/Documents, say) across rebuilds; an ad-hoc signature
# changes every build, so each install would be a new app to it and asking
# again, or refusing. ZTERM_SIGN_IDENTITY picks one; else the first valid
# Apple Development or Developer ID identity, else ad-hoc.
identity=${ZTERM_SIGN_IDENTITY:-$(security find-identity -v -p codesigning 2>/dev/null |
  grep -E '"(Apple Development|Developer ID Application)' | grep -v CSSMERR | head -1 | awk '{print $2}')}
if [ -n "$identity" ] && codesign --force --deep --sign "$identity" "$staging" >/dev/null 2>&1; then
  echo "signed with $identity"
else
  codesign --force --deep --sign - "$staging" >/dev/null 2>&1 || true
  echo "signed ad hoc: macOS asks again for its permissions after each install"
fi
# The name's capital T is a word break to Spotlight, which matches names
# by their words' starts: "term" finds it, as "time" finds FaceTime. Its
# earlier name (zterm.app) goes: the same path on a case-insensitive disk.
rm -rf "$dest/zterm.app" "$app"
mv "$staging" "$app"
/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister -f "$app"
mdimport "$app" 2>/dev/null || true
echo "installed $app"
if [ "${ZTERM_DEFAULTS:-0}" = 1 ]; then
  swift scripts/zterm-defaults.swift "$app"
fi

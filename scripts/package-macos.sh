#!/bin/bash
# Build a local LESS.app, and optionally a zip + DMG. No upload, Developer ID signing, or notarization.
set -euo pipefail

repo=$(cd "$(dirname "$0")/.." && pwd)
app_only=0
build=1
while [ "$#" -gt 0 ]; do
  case "$1" in
    --app-only) app_only=1 ;;
    --no-build) build=0 ;;
    *) printf 'Unknown option: %s\n' "$1" >&2; exit 2 ;;
  esac
  shift
done

if [ "$(uname -s)" != "Darwin" ]; then
  printf 'macOS packaging requires Darwin.\n' >&2
  exit 1
fi
for tool in cargo codesign ditto hdiutil iconutil lipo sips; do
  command -v "$tool" >/dev/null 2>&1 || { printf 'Missing required tool: %s\n' "$tool" >&2; exit 1; }
done

if [ "$build" -eq 1 ]; then
  (cd "$repo" && cargo build --release -p less-app)
fi

target=${CARGO_TARGET_DIR:-target}
case "$target" in /*) ;; *) target="$repo/$target" ;; esac
binary="$target/release/less-rs"
[ -x "$binary" ] || { printf 'Missing release binary: %s\n' "$binary" >&2; exit 1; }
# LESS_VERSION (e.g. from a release tag) overrides the workspace version.
version=${LESS_VERSION:-$(sed -n 's/^version = "\([^"]*\)"/\1/p' "$repo/Cargo.toml")}
[ -n "$version" ] || { printf 'Could not read workspace version.\n' >&2; exit 1; }

app="$repo/build/LESS.app"
out="$repo/build/dist"
contents="$app/Contents"
rm -rf "$app"
mkdir -p "$contents/MacOS" "$contents/Resources" "$out"
install -m 755 "$binary" "$contents/MacOS/LESS"
cp "$repo/LICENSE" "$contents/Resources/LICENSE"
cp "$repo/crates/less-app/assets/Phosphor-LICENSE.txt" "$contents/Resources/Phosphor-LICENSE.txt"

work=$(mktemp -d "${TMPDIR:-/tmp}/less-rs-package.XXXXXX")
trap 'rm -rf "$work"' EXIT
iconset="$work/less.iconset"
mkdir -p "$iconset"
for size in 16 32 128 256 512; do
  double=$((size * 2))
  sips -z "$size" "$size" "$repo/crates/less-app/assets/icon.png" --out "$iconset/icon_${size}x${size}.png" >/dev/null
  sips -z "$double" "$double" "$repo/crates/less-app/assets/icon.png" --out "$iconset/icon_${size}x${size}@2x.png" >/dev/null
done
iconutil -c icns "$iconset" -o "$contents/Resources/less.icns"

cat >"$contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>CFBundleDevelopmentRegion</key><string>en</string>
  <key>CFBundleDisplayName</key><string>LESS</string>
  <key>CFBundleExecutable</key><string>LESS</string>
  <key>CFBundleIconFile</key><string>less</string>
  <key>CFBundleIdentifier</key><string>com.oguz-dev.less-rs</string>
  <key>CFBundleName</key><string>LESS</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>$version</string>
  <key>CFBundleVersion</key><string>$version</string>
  <key>LSMinimumSystemVersion</key><string>12.0</string>
  <key>NSHighResolutionCapable</key><true/>
</dict></plist>
PLIST

codesign --force --sign - "$app"
codesign --verify --strict "$app"
printf 'Built %s (ad-hoc signed; not notarized).\n' "$app"
[ "$app_only" -eq 0 ] || exit 0

archs=$(lipo -archs "$binary")
case "$archs" in
  *arm64*x86_64*|*x86_64*arm64*) arch=universal ;;
  *) arch=$archs ;;
esac
name="LESS-$version-macos-$arch"
readme="$work/README.txt"
cat >"$readme" <<'README'
LESS — local development build

Drag LESS.app to Applications or run it directly.
This package is ad-hoc signed and is not notarized. macOS may ask for confirmation.
No database drivers are bundled; optional drivers require separate approval and verification.
README
stage="$work/dmg"
mkdir -p "$stage"
ditto "$app" "$stage/LESS.app"
cp "$readme" "$stage/README.txt"
ln -s /Applications "$stage/Applications"
rm -f "$out/$name.zip" "$out/$name.dmg"
ditto -c -k --keepParent "$app" "$out/$name.zip"
hdiutil create -quiet -volname "LESS $version" -srcfolder "$stage" -ov -format UDZO "$out/$name.dmg"
printf 'Built %s and %s (not notarized; no upload performed).\n' "$out/$name.zip" "$out/$name.dmg"

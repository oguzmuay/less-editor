#!/bin/sh
# package-macos.sh <LESS.app> <outdir>: release archives LESS-<version>-macos-<arch>.zip and .dmg
# (LESS.app + lessed + install.sh + README.txt).
# Optional, only when the env vars are set:
#   LESS_SIGN_IDENTITY            Developer ID identity in the keychain -> hardened-runtime re-sign (else ad-hoc stays)
#   LESS_NOTARY_APPLE_ID, LESS_NOTARY_TEAM_ID, LESS_NOTARY_PASSWORD -> notarytool submit + staple (needs signing)
set -eu
app=$1 out=$2
repo=$(cd "$(dirname "$0")/../.." && pwd)
version=$(/usr/libexec/PlistBuddy -c "Print :CFBundleShortVersionString" "$app/Contents/Info.plist")
archs=$(lipo -archs "$app/Contents/MacOS/LESS")
case $archs in *x86_64*arm64* | *arm64*x86_64*) arch=universal ;; *) arch=$archs ;; esac
name="LESS-$version-macos-$arch"
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
stage="$work/$name"
mkdir -p "$stage" "$out"

ditto "$app" "$stage/LESS.app"
install -m 755 "$repo/app/scripts/lessed" "$stage/lessed"
install -m 755 "$repo/app/less/install-macos.sh" "$stage/install.sh"
cp "$repo/app/less/README-macos.txt" "$stage/README.txt"

notarize() {
  [ -n "${LESS_NOTARY_APPLE_ID:-}" ] || return 0
  xcrun notarytool submit "$1" --wait \
    --apple-id "$LESS_NOTARY_APPLE_ID" --team-id "$LESS_NOTARY_TEAM_ID" --password "$LESS_NOTARY_PASSWORD"
}

if [ -n "${LESS_SIGN_IDENTITY:-}" ]; then
  # shortcut: --deep gives nested Sparkle helpers the app entitlements too; sign inside-out if notarization complains.
  codesign --force --deep --options runtime --timestamp \
    --entitlements "$repo/app/ghostty/macos/Ghostty.entitlements" --sign "$LESS_SIGN_IDENTITY" "$stage/LESS.app"
  codesign --verify --deep --strict "$stage/LESS.app"
  if [ -n "${LESS_NOTARY_APPLE_ID:-}" ]; then
    ditto -c -k --keepParent "$stage/LESS.app" "$work/notarize.zip"
    notarize "$work/notarize.zip"
    xcrun stapler staple "$stage/LESS.app"
  fi
fi

rm -f "$out/$name.zip" "$out/$name.dmg"
ditto -c -k --keepParent "$stage" "$out/$name.zip"
ln -s /Applications "$stage/Applications"
hdiutil create -quiet -volname "LESS $version" -srcfolder "$stage" -ov -format UDZO "$out/$name.dmg"
if [ -n "${LESS_SIGN_IDENTITY:-}" ]; then
  codesign --force --timestamp --sign "$LESS_SIGN_IDENTITY" "$out/$name.dmg"
  [ -z "${LESS_NOTARY_APPLE_ID:-}" ] || { notarize "$out/$name.dmg" && xcrun stapler staple "$out/$name.dmg"; }
fi
echo "Built $out/$name.zip $out/$name.dmg"

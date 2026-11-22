#!/usr/bin/env bash
# Build Voxa.app (universal: Apple Silicon + Intel) and a .dmg into target/dist. Runs on macOS.
set -euo pipefail
cd "$(dirname "$0")/../.."
VERSION=$(grep -m1 '^version' Cargo.toml | cut -d'"' -f2)
TARGETS="aarch64-apple-darwin x86_64-apple-darwin"
for t in $TARGETS; do
  rustup target add "$t" >/dev/null
  cargo build --release --target "$t" -p voxa-desktop -p voxa-signin
done

APP=target/dist/Voxa.app
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
lipo -create target/aarch64-apple-darwin/release/voxa-desktop target/x86_64-apple-darwin/release/voxa-desktop -output "$APP/Contents/MacOS/Voxa"
lipo -create target/aarch64-apple-darwin/release/voxa-signin target/x86_64-apple-darwin/release/voxa-signin -output "$APP/Contents/MacOS/voxa-signin"

# ONNX runtime for silence trimming: Microsoft's official package, both architectures.
ORT=target/ort-mac
if [ ! -f "$ORT/libonnxruntime.dylib" ]; then
  mkdir -p "$ORT"
  curl -sSfL -o "$ORT/ort.zip" https://api.nuget.org/v3-flatcontainer/microsoft.ml.onnxruntime/1.22.0/microsoft.ml.onnxruntime.1.22.0.nupkg
  (cd "$ORT" && unzip -oq ort.zip 'runtimes/osx-*/native/libonnxruntime.dylib')
  ls "$ORT"/runtimes/*/native/
  lipo -create "$ORT"/runtimes/osx-*/native/libonnxruntime.dylib -output "$ORT/libonnxruntime.dylib" 2>/dev/null \
    || cp "$ORT"/runtimes/osx-arm64/native/libonnxruntime.dylib "$ORT/libonnxruntime.dylib"
fi
cp "$ORT/libonnxruntime.dylib" "$APP/Contents/MacOS/"

# Icon.
ICONSET=target/Voxa.iconset
rm -rf "$ICONSET"; mkdir -p "$ICONSET"
for s in 16 32 128 256 512; do
  sips -z $s $s desktop/app/assets/icon.png --out "$ICONSET/icon_${s}x${s}.png" >/dev/null
  d=$((s * 2)); sips -z $d $d desktop/app/assets/icon.png --out "$ICONSET/icon_${s}x${s}@2x.png" >/dev/null
done
iconutil -c icns "$ICONSET" -o "$APP/Contents/Resources/Voxa.icns"

sed "s/VERSION/$VERSION/g" desktop/macos/Info.plist > "$APP/Contents/Info.plist"

# Ad-hoc signature so macOS remembers the microphone and Accessibility grants.
codesign --force --sign - --options runtime "$APP/Contents/MacOS/libonnxruntime.dylib"
codesign --force --sign - --options runtime --entitlements desktop/macos/entitlements.plist "$APP/Contents/MacOS/voxa-signin"
codesign --force --sign - --options runtime --entitlements desktop/macos/entitlements.plist "$APP"
codesign --verify --deep --strict "$APP"
lipo -archs "$APP/Contents/MacOS/Voxa"

DMG="target/dist/Voxa-$VERSION-mac.dmg"
rm -f "$DMG"
STAGE=target/dmg-stage
rm -rf "$STAGE"; mkdir -p "$STAGE"; cp -R "$APP" "$STAGE/"; ln -s /Applications "$STAGE/Applications"
hdiutil create -volname "Voxa" -srcfolder "$STAGE" -ov -format UDZO "$DMG" >/dev/null
ls -lh "$DMG"

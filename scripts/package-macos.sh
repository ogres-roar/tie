#!/bin/sh
set -eu

ROOT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$ROOT_DIR"

APP_DIR="$ROOT_DIR/target/macos/Tie.app"
CONTENTS_DIR="$APP_DIR/Contents"
MACOS_DIR="$CONTENTS_DIR/MacOS"
RESOURCES_DIR="$CONTENTS_DIR/Resources"
ICONSET_DIR="$ROOT_DIR/target/macos/Tie.iconset"

cargo build --release

mkdir -p "$MACOS_DIR" "$RESOURCES_DIR" "$ICONSET_DIR"
cp "$ROOT_DIR/target/release/tie" "$MACOS_DIR/tie"
cp "$ROOT_DIR/packaging/macos/Info.plist" "$CONTENTS_DIR/Info.plist"

for size in 16 32 128 256 512; do
    sips -z "$size" "$size" "$ROOT_DIR/assets/tie-icon.png" \
        --out "$ICONSET_DIR/icon_${size}x${size}.png" >/dev/null
    doubled_size=$((size * 2))
    sips -z "$doubled_size" "$doubled_size" "$ROOT_DIR/assets/tie-icon.png" \
        --out "$ICONSET_DIR/icon_${size}x${size}@2x.png" >/dev/null
done

iconutil -c icns "$ICONSET_DIR" -o "$RESOURCES_DIR/Tie.icns"

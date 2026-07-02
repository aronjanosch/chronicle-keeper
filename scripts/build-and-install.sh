#!/usr/bin/env bash
# Build the macOS app and install it into /Applications, replacing any existing copy.
set -euo pipefail

cd "$(dirname "$0")/.."

APP_NAME="chronicle-keeper.app"
BUNDLE_DIR="target/release/bundle/macos"
DEST="/Applications/$APP_NAME"

echo "==> cargo tauri build"
cargo tauri build

SRC="$BUNDLE_DIR/$APP_NAME"
if [ ! -d "$SRC" ]; then
  echo "error: built app not found at $SRC" >&2
  exit 1
fi

if [ -d "$DEST" ]; then
  echo "==> removing existing $DEST"
  rm -rf "$DEST"
fi

echo "==> installing to $DEST"
cp -R "$SRC" "$DEST"

echo "==> done: $DEST"

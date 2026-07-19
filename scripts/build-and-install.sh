#!/usr/bin/env bash
# Build the macOS app and install it into /Applications, replacing any existing copy.
set -euo pipefail

cd "$(dirname "$0")/.."

APP_NAME="chronicle-keeper.app"
BUNDLE_DIR="target/release/bundle/macos"
DEST="/Applications/$APP_NAME"

# Mirror release.yml's "Sync version to tag" step: the committed tauri.conf.json
# version is a stale placeholder (CI always overwrites it from the release tag),
# so a plain local build would report that stale version and misfire the
# update-available check against every real release. Use the latest reachable
# tag instead so local builds report something meaningful.
TAG="$(git describe --tags --abbrev=0 2>/dev/null || true)"
if [ -n "$TAG" ]; then
  VER="${TAG#v}"
  VER="${VER%%-*}"
  echo "==> syncing bundle version to $VER (from tag $TAG)"
  # CI's runner is thrown away after each build, so its version edit never
  # lingers; a local checkout isn't, so restore both files on exit either way
  # rather than leaving a version-only diff in the working tree.
  trap 'git checkout -- src-tauri/tauri.conf.json src-tauri/Cargo.toml Cargo.lock' EXIT
  jq --arg v "$VER" '.version = $v' src-tauri/tauri.conf.json > src-tauri/tauri.conf.tmp
  mv src-tauri/tauri.conf.tmp src-tauri/tauri.conf.json
  sed -i.bak -E "s/^version = \"[^\"]*\"/version = \"$VER\"/" src-tauri/Cargo.toml
  rm -f src-tauri/Cargo.toml.bak
else
  echo "==> no git tag found, leaving tauri.conf.json version as-is"
fi

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

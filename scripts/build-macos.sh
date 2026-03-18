#!/usr/bin/env bash
# Build Grove as a distributable macOS app for both Intel and Apple Silicon.
# Produces:
#   dist/Grove_aarch64.dmg  (Apple Silicon)
#   dist/Grove_x86_64.dmg   (Intel)
#   dist/Grove_universal.dmg (fat binary — runs on both)
#
# Usage:
#   ./scripts/build-macos.sh           # all three
#   ./scripts/build-macos.sh --arm     # Apple Silicon only
#   ./scripts/build-macos.sh --intel   # Intel only
#   ./scripts/build-macos.sh --universal # universal binary only

set -euo pipefail

# Ensure ~/.cargo/bin is on PATH (rustup/cargo may not be in non-login shells)
export PATH="$HOME/.cargo/bin:$PATH"

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
GUI_DIR="$REPO_ROOT/crates/grove-gui"
BUNDLE_DIR="$REPO_ROOT/target"
DIST_DIR="$REPO_ROOT/dist"
VERSION=$(grep '"version"' "$GUI_DIR/src-tauri/tauri.conf.json" | head -1 | sed 's/.*"version": "\(.*\)".*/\1/')

ARM_TARGET="aarch64-apple-darwin"
INTEL_TARGET="x86_64-apple-darwin"

BUILD_ARM=true
BUILD_INTEL=true
BUILD_UNIVERSAL=true

for arg in "$@"; do
  case "$arg" in
    --arm)       BUILD_ARM=true;  BUILD_INTEL=false; BUILD_UNIVERSAL=false ;;
    --intel)     BUILD_ARM=false; BUILD_INTEL=true;  BUILD_UNIVERSAL=false ;;
    --universal) BUILD_ARM=false; BUILD_INTEL=false; BUILD_UNIVERSAL=true  ;;
  esac
done

# ── helpers ────────────────────────────────────────────────────────────────────

log() { echo "==> $*"; }

require() {
  if ! command -v "$1" &>/dev/null; then
    echo "ERROR: '$1' not found. $2"
    exit 1
  fi
}

# ── preflight ─────────────────────────────────────────────────────────────────

require rustup  "Install from https://rustup.rs"
require npm     "Install Node.js from https://nodejs.org"
require cargo   "Install from https://rustup.rs"

log "Grove v${VERSION} — macOS build"
log "Repo: $REPO_ROOT"

mkdir -p "$DIST_DIR"

# ── install Rust targets ───────────────────────────────────────────────────────

if $BUILD_ARM || $BUILD_UNIVERSAL; then
  log "Adding Rust target: $ARM_TARGET"
  rustup target add "$ARM_TARGET"
fi

if $BUILD_INTEL || $BUILD_UNIVERSAL; then
  log "Adding Rust target: $INTEL_TARGET"
  rustup target add "$INTEL_TARGET"
fi

# ── signing key ───────────────────────────────────────────────────────────────
# Required for tauri-plugin-updater to sign update artifacts.
# Store the private key in .grove-signing-key (gitignored) at the repo root.
SIGNING_KEY_FILE="$REPO_ROOT/.grove-signing-key"
if [[ -f "$SIGNING_KEY_FILE" ]]; then
  export TAURI_SIGNING_PRIVATE_KEY="$(cat "$SIGNING_KEY_FILE")"
  export TAURI_SIGNING_PRIVATE_KEY_PASSWORD=""
else
  echo "WARNING: $SIGNING_KEY_FILE not found — update artifacts will not be signed."
  echo "         Run: ./scripts/build-macos.sh  (will skip signing)"
fi

# ── install npm deps ───────────────────────────────────────────────────────────

log "Installing npm dependencies"
cd "$GUI_DIR"
npm install

TAURI="$GUI_DIR/node_modules/.bin/tauri"

# ── build Apple Silicon ────────────────────────────────────────────────────────

if $BUILD_ARM; then
  log "Building for Apple Silicon ($ARM_TARGET)"
  "$TAURI" build --target "$ARM_TARGET"

  ARM_DMG=$(find "$BUNDLE_DIR/$ARM_TARGET/release/bundle/dmg" -name "*.dmg" | head -1)
  if [[ -z "$ARM_DMG" ]]; then
    echo "ERROR: ARM DMG not found after build"
    exit 1
  fi
  cp "$ARM_DMG" "$DIST_DIR/Grove_${VERSION}_aarch64.dmg"
  log "Apple Silicon DMG: $DIST_DIR/Grove_${VERSION}_aarch64.dmg"
fi

# ── build Intel ───────────────────────────────────────────────────────────────

if $BUILD_INTEL; then
  log "Building for Intel ($INTEL_TARGET)"
  "$TAURI" build --target "$INTEL_TARGET"

  INTEL_DMG=$(find "$BUNDLE_DIR/$INTEL_TARGET/release/bundle/dmg" -name "*.dmg" | head -1)
  if [[ -z "$INTEL_DMG" ]]; then
    echo "ERROR: Intel DMG not found after build"
    exit 1
  fi
  cp "$INTEL_DMG" "$DIST_DIR/Grove_${VERSION}_x86_64.dmg"
  log "Intel DMG: $DIST_DIR/Grove_${VERSION}_x86_64.dmg"
fi

# ── build Universal (fat binary) ──────────────────────────────────────────────

if $BUILD_UNIVERSAL; then
  log "Building Universal binary (Intel + Apple Silicon)"
  "$TAURI" build --target universal-apple-darwin

  UNIV_DMG=$(find "$BUNDLE_DIR/universal-apple-darwin/release/bundle/dmg" -name "*.dmg" | head -1)
  if [[ -z "$UNIV_DMG" ]]; then
    echo "ERROR: Universal DMG not found after build"
    exit 1
  fi
  cp "$UNIV_DMG" "$DIST_DIR/Grove_${VERSION}_universal.dmg"
  log "Universal DMG: $DIST_DIR/Grove_${VERSION}_universal.dmg"
fi

# ── summary ───────────────────────────────────────────────────────────────────

echo ""
echo "Build complete. Artifacts in $DIST_DIR:"
ls -lh "$DIST_DIR"/*.dmg 2>/dev/null || true

echo ""
echo "To share with your friend:"
echo "  - Send them the .dmg file"
echo "  - They double-click it and drag Grove.app to /Applications"
echo "  - If Gatekeeper blocks it: right-click → Open, or run:"
echo "      xattr -d com.apple.quarantine /Applications/Grove.app"

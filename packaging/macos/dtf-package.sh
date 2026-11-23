#!/usr/bin/env bash
# DTF Craft (2026, Stéphane Glacet): DTF Craft's macOS package, from PhotoCraft's package.sh.
#
# Builds, signs and (optionally) notarizes:
#
#   $DIST/dtf-craft-macos-<arch>.dmg    DTF Craft.app (with dtfcraft-cli inside) on a drag-to-Applications DMG
#
# Usage: packaging/macos/dtf-package.sh [--arch universal|aarch64|x86_64] [--skip-build]
#
# Signing (env):
#   MACOS_SIGN_IDENTITY   codesign identity (name or SHA-1). Default "-" = ad-hoc: the app runs, but
#                         on another Mac Gatekeeper asks the user to allow it once (System Settings ›
#                         Privacy & Security › Open Anyway). import-cert.sh sets it from a certificate.
#   MACOS_KEYCHAIN        keychain holding the identity (optional)
# Notarization (env; all three needed, and a real identity):
#   APPLE_ID, APPLE_PASSWORD (app-specific password), APPLE_TEAM_ID
set -euo pipefail
# shellcheck source=../env.sh
. "$(dirname "${BASH_SOURCE[0]}")/../env.sh"
HERE="$ROOT/packaging/macos"

ARCH=universal
SKIP_BUILD=0
while [[ $# -gt 0 ]]; do
  case "$1" in
    --arch) ARCH="$2"; shift 2 ;;
    --skip-build) SKIP_BUILD=1; shift ;;
    -h | --help) sed -n '2,18p' "$0"; exit 0 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done
case "$ARCH" in
  universal) TARGETS=(aarch64-apple-darwin x86_64-apple-darwin) ;;
  aarch64) TARGETS=(aarch64-apple-darwin) ;;
  x86_64) TARGETS=(x86_64-apple-darwin) ;;
  *) echo "unknown --arch $ARCH" >&2; exit 2 ;;
esac

# Keep in sync with LSMinimumSystemVersion in dtf-Info.plist.in (macOS 11: the first release on
# Apple silicon, with the Metal feature set wgpu targets).
export MACOSX_DEPLOYMENT_TARGET=11.0
IDENTITY="${MACOS_SIGN_IDENTITY:--}"
# DTF Craft's own version (shown in the app), not the PhotoCraft base's (the workspace version).
DTF_VERSION="$(sed -n 's/^pub const DTF_CRAFT_VERSION: &str = "\(.*\)";/\1/p' "$ROOT/crates/engine/src/build_info.rs")"
if [[ -z "$DTF_VERSION" ]]; then
  echo "error: DTF_CRAFT_VERSION not found in crates/engine/src/build_info.rs" >&2
  exit 1
fi
APP_NAME="DTF Craft"
WORK="$CARGO_TARGET_DIR/macos-package"
APP="$WORK/$APP_NAME.app"
DMG="$DIST/dtf-craft-macos-$ARCH.dmg"

NOTARIZE=0
if [[ "$IDENTITY" = "-" ]]; then
  warn "macOS: ad-hoc signing (no MACOS_SIGN_IDENTITY); the DMG is not notarized"
elif [[ -n "${APPLE_ID:-}" ]] && [[ -n "${APPLE_PASSWORD:-}" ]] && [[ -n "${APPLE_TEAM_ID:-}" ]]; then
  NOTARIZE=1
else
  warn "macOS: APPLE_ID / APPLE_PASSWORD / APPLE_TEAM_ID incomplete; signed but not notarized"
fi

echo "==> DTF Craft $DTF_VERSION (PhotoCraft $VERSION) for macOS ($ARCH), identity: $IDENTITY, notarize: $NOTARIZE"

# ---- build (the same packages and features as the Windows build) -------------------------------
if [[ "$SKIP_BUILD" = 0 ]]; then
  args=()
  for t in "${TARGETS[@]}"; do args+=(--target "$t"); done
  (cd "$ROOT" && cargo build --release -p photocraft -p photocraft-cli "${args[@]}")
fi

rm -rf "$WORK"
mkdir -p "$WORK/bin"
for bin in photocraft photocraft-cli; do
  inputs=()
  for t in "${TARGETS[@]}"; do inputs+=("$CARGO_TARGET_DIR/$t/release/$bin"); done
  lipo -create -output "$WORK/bin/$bin" "${inputs[@]}"
  lipo -info "$WORK/bin/$bin"
done

# ---- signing helpers (as package.sh) -----------------------------------------------------------
sign() {
  # Hardened runtime + secure timestamp for a real identity; ad-hoc can't be timestamped.
  local ts=(--timestamp)
  [[ "$IDENTITY" = "-" ]] && ts=(--timestamp=none)
  local kc=()
  [[ -n "${MACOS_KEYCHAIN:-}" ]] && kc=(--keychain "$MACOS_KEYCHAIN")
  codesign --force --sign "$IDENTITY" ${kc[@]+"${kc[@]}"} "${ts[@]}" "$@"
}

notarize() {
  local file="$1" out id status
  echo "==> notarizing $(basename "$file") (this can take a few minutes)"
  out="$(xcrun notarytool submit "$file" --apple-id "$APPLE_ID" --password "$APPLE_PASSWORD" \
    --team-id "$APPLE_TEAM_ID" --wait --timeout 1h --output-format json)" || true
  echo "$out"
  id="$(printf '%s' "$out" | plutil -extract id raw -o - - 2>/dev/null || true)"
  status="$(printf '%s' "$out" | plutil -extract status raw -o - - 2>/dev/null || true)"
  if [[ "$status" != "Accepted" ]]; then
    if [[ -n "$id" ]]; then
      xcrun notarytool log "$id" --apple-id "$APPLE_ID" --password "$APPLE_PASSWORD" --team-id "$APPLE_TEAM_ID" || true
    fi
    echo "error: notarization of $(basename "$file") failed (status: ${status:-unknown})" >&2
    exit 1
  fi
}

# Stapling right after notarization can drop the connection (error 68): retry a few times.
staple() {
  local file="$1" attempt
  for attempt in 1 2 3 4 5; do
    if xcrun stapler staple "$file" && xcrun stapler validate "$file"; then
      return
    fi
    if [[ "$attempt" = 5 ]]; then
      echo "error: stapling $(basename "$file") failed after $attempt attempts" >&2
      exit 1
    fi
    echo "stapling $(basename "$file") failed (attempt $attempt); retrying in $((attempt * 20))s" >&2
    sleep $((attempt * 20))
  done
}

# ---- DTF Craft.app -----------------------------------------------------------------------------
echo "==> assembling $APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
# Executable and icon carry the display name (CFBundleExecutable / CFBundleIconFile).
cp "$WORK/bin/photocraft" "$APP/Contents/MacOS/$APP_NAME"
cp "$WORK/bin/photocraft-cli" "$APP/Contents/MacOS/dtfcraft-cli"
cp "$ROOT/assets/app-icon/photocraft.icns" "$APP/Contents/Resources/$APP_NAME.icns"
# Licences and read-me, as in the Windows zip (inside the bundle, before it is signed).
cp "$ROOT/LICENSE-MIT" "$ROOT/LICENSE-APACHE" "$ROOT/NOTICE" "$ROOT/MODIFICATIONS.md" "$ROOT/ATTRIBUTION.md" "$APP/Contents/Resources/"
cp "$ROOT/crates/algo/models/LICENSE" "$APP/Contents/Resources/LICENSE-Real-ESRGAN.txt"
cp "$ROOT/DTF-CRAFT.md" "$APP/Contents/Resources/LISEZ-MOI.md"
sed -e "s/@SHORT_VERSION@/$DTF_VERSION/g" -e "s/@PHOTOCRAFT_VERSION@/$VERSION/g" \
  -e "s/@BUILD_SHA@/${PHOTOCRAFT_BUILD_SHA:-unknown}/g" \
  "$HERE/dtf-Info.plist.in" >"$APP/Contents/Info.plist"
plutil -lint "$APP/Contents/Info.plist"
printf 'APPL????' >"$APP/Contents/PkgInfo"

# Sign inside-out: nested code first, then the bundle itself (no --deep on the final signature).
sign --options runtime --identifier fr.dtfcraft.cli "$APP/Contents/MacOS/dtfcraft-cli"
sign --options runtime --entitlements "$HERE/entitlements.plist" "$APP/Contents/MacOS/$APP_NAME"
sign --options runtime --entitlements "$HERE/entitlements.plist" "$APP"
codesign --verify --strict --deep --verbose=2 "$APP"

if [[ "$NOTARIZE" = 1 ]]; then
  ditto -c -k --keepParent "$APP" "$WORK/DTFCraft-notarize.zip"
  notarize "$WORK/DTFCraft-notarize.zip"
  staple "$APP"
  spctl --assess --type execute -vvv "$APP"
fi

# ---- DMG ---------------------------------------------------------------------------------------
echo "==> building $DMG"
STAGE="$WORK/dmg"
mkdir -p "$STAGE"
ditto "$APP" "$STAGE/$APP_NAME.app"
ln -s /Applications "$STAGE/Applications"
# Finder window layout: background, icon size and positions (packaging/macos/dmg/README-DTF.md).
mkdir -p "$STAGE/.background"
cp "$HERE/dmg/dtf-background.tiff" "$STAGE/.background/background.tiff"
cp "$HERE/dmg/dtf-layout.DS_Store" "$STAGE/.DS_Store"
rm -f "$DMG" "$WORK/raw.dmg"
# makehybrid + convert: no attached device (flaky on CI runners). The volume name has no version:
# .DS_Store finds the background through an alias that includes it (dtf-generate.py).
hdiutil makehybrid -hfs -hfs-volume-name "$APP_NAME" -hfs-openfolder "$STAGE" -o "$WORK/raw.dmg" "$STAGE"
hdiutil convert "$WORK/raw.dmg" -format UDZO -imagekey zlib-level=9 -o "$DMG"
rm -f "$WORK/raw.dmg"
sign "$DMG"
codesign --verify --strict --verbose=2 "$DMG"
if [[ "$NOTARIZE" = 1 ]]; then
  notarize "$DMG"
  staple "$DMG"
  spctl --assess --type open --context context:primary-signature -vvv "$DMG"
fi

"$APP/Contents/MacOS/dtfcraft-cli" --version
echo "==> done"
ls -lh "$DMG"

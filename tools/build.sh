#!/bin/sh
set -eu
ROOT=$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd -P)
cd "$ROOT"
ODIN=${ODIN:-odin}
PROFILE=debug
CARGO_PROFILE=
# Keep Odin's main executable compatible with the declared application floor.
MACOS_DEPLOYMENT_TARGET=13.0
if [ "${1-}" = "--release" ]; then PROFILE=release; CARGO_PROFILE=--release; fi
mkdir -p target
cargo build -p punks-caliber $CARGO_PROFILE
RPATH=
SDL3_FLAGS=
REPO_ROOT="$ROOT/vendor/alicorn"
. "$REPO_ROOT/tools/common.sh"
ACCESSKIT=$(alicorn_accesskit_linker_flags)
# Alicorn's bootstrap reports paths relative to the vendored checkout.
ACCESSKIT=$(printf '%s' "$ACCESSKIT" | sed "s@-L.deps/@-L$REPO_ROOT/.deps/@")
case "$(uname -s)" in
 Darwin)
  install_name_tool -id @rpath/libpunks_caliber.dylib "target/$PROFILE/libpunks_caliber.dylib"
  RPATH='-Wl,-rpath,@executable_path'
  SDL3_PREFIX=${SDL3_PREFIX:-$(brew --prefix sdl3)}
  SDL3_VERSION=$(PKG_CONFIG_PATH="$SDL3_PREFIX/lib/pkgconfig" pkg-config --modversion sdl3)
  SDL3_RUNTIME=$(find "$SDL3_PREFIX/lib" -maxdepth 1 -type f -name 'libSDL3*.dylib' | sort | head -n 1)
  if [ -z "$SDL3_RUNTIME" ]; then echo "SDL3 runtime not found under $SDL3_PREFIX/lib" >&2; exit 1; fi
  SDL3_FLAGS="-L$SDL3_PREFIX/lib"
  ;;
 Linux) RPATH='-Wl,-rpath,$ORIGIN' ;;
 *) echo 'The Punks native build helper supports macOS and Linux only.' >&2; exit 1 ;;
esac
set -- build punks-standalone/odin "-collection:alicorn=$REPO_ROOT" "-collection:alikit=$ROOT/vendor/alicorn-kit" "-out:$ROOT/target/$PROFILE/punks"
if [ "$(uname -s)" = 'Darwin' ]; then
 set -- "$@" "-minimum-os-version:$MACOS_DEPLOYMENT_TARGET"
fi
alicorn_odin_with_linker_flags "$ODIN" "-L$ROOT/target/$PROFILE $RPATH $ACCESSKIT $SDL3_FLAGS" "$@"
printf 'Built %s\n' "$ROOT/target/$PROFILE/punks"
if [ "$(uname -s)" = 'Darwin' ]; then
 EXECUTABLE_MINIMUM=$(xcrun vtool -show-build "target/$PROFILE/punks" | awk '/^[[:space:]]+minos / {print $2; exit}')
 if [ "$EXECUTABLE_MINIMUM" != "$MACOS_DEPLOYMENT_TARGET" ]; then
  echo "Odin executable minimum is $EXECUTABLE_MINIMUM; expected $MACOS_DEPLOYMENT_TARGET" >&2
  exit 1
 fi
 # Bundled dynamic libraries can require a newer OS than the application code.
 BUNDLE_MINIMUM=$(python3 - "$MACOS_DEPLOYMENT_TARGET" "target/$PROFILE/punks" "target/$PROFILE/libpunks_caliber.dylib" "$SDL3_RUNTIME" <<'PY'
import re
import subprocess
import sys

version = lambda value: tuple(int(part) for part in value.split("."))
minimum = version(sys.argv[1])
for image in sys.argv[2:]:
    output = subprocess.check_output(["xcrun", "vtool", "-show-build", image], text=True)
    versions = re.findall(r"(?m)^\s*minos\s+([0-9]+(?:\.[0-9]+){0,2})\s*$", output)
    if not versions:
        raise SystemExit(f"No LC_BUILD_VERSION minimum found in {image}")
    minimum = max(minimum, *(version(item) for item in versions))
print(".".join(str(part) for part in minimum))
PY
 )
 PACKAGE_OUT="$ROOT/target/$PROFILE/punks-package"
 cat > "target/$PROFILE/punks-package.json" <<EOF
{
  "name": "Punks",
  "version": "0.3.0",
  "bundleIdentifier": "com.punks.foundation",
  "executableName": "punks",
  "minimumMacOSVersion": "$BUNDLE_MINIMUM",
  "privateLibraries": ["libpunks_caliber.dylib"]
}
EOF
 rm -rf "$PACKAGE_OUT"
 python3 vendor/alicorn/tools/package_macos.py \
  --executable "target/$PROFILE/punks" \
  --metadata "target/$PROFILE/punks-package.json" \
  --linkage shared \
  --sdl-version "$SDL3_VERSION" \
  --sdl-license "$SDL3_PREFIX/share/licenses/SDL3/LICENSE.txt" \
  --sdl-runtime "$SDL3_RUNTIME" \
  --accesskit-license-apache "$REPO_ROOT/.deps/accesskit/accesskit-c-0.23.1/LICENSE-APACHE" \
  --accesskit-license-mit "$REPO_ROOT/.deps/accesskit/accesskit-c-0.23.1/LICENSE-MIT" \
  --output "$PACKAGE_OUT"
 printf 'Built %s\n' "$PACKAGE_OUT/Punks.app"
 printf 'Minimum macOS version: %s\n' "$BUNDLE_MINIMUM"
fi

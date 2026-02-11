#!/usr/bin/env bash
set -euo pipefail
IFS=$'\n\t'

usage() {
  cat <<'EOF'
Usage: scripts/release-build-artifacts.sh --version <vX.Y.Z[-prerelease]> --os <linux|darwin> --arch <amd64|arm64> [--out-dir <dir>]

Builds Rust release binaries and packages:
  forge, forged, forge-agent-runner, fmail

Archive format:
  forge_<version-no-v>_<os>_<arch>.tar.gz
EOF
}

VERSION=""
OS=""
ARCH=""
OUT_DIR="dist"

while [ $# -gt 0 ]; do
  case "$1" in
    --version)
      [ $# -ge 2 ] || {
        echo "error: --version requires a value" >&2
        exit 1
      }
      VERSION="$2"
      shift 2
      ;;
    --os)
      [ $# -ge 2 ] || {
        echo "error: --os requires a value" >&2
        exit 1
      }
      OS="$2"
      shift 2
      ;;
    --arch)
      [ $# -ge 2 ] || {
        echo "error: --arch requires a value" >&2
        exit 1
      }
      ARCH="$2"
      shift 2
      ;;
    --out-dir)
      [ $# -ge 2 ] || {
        echo "error: --out-dir requires a value" >&2
        exit 1
      }
      OUT_DIR="$2"
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "error: unknown argument: $1" >&2
      usage
      exit 1
      ;;
  esac
done

[ -n "$VERSION" ] || {
  echo "error: --version is required" >&2
  usage
  exit 1
}
[ -n "$OS" ] || {
  echo "error: --os is required" >&2
  usage
  exit 1
}
[ -n "$ARCH" ] || {
  echo "error: --arch is required" >&2
  usage
  exit 1
}

case "$OS/$ARCH" in
  linux/amd64) TARGET_TRIPLE="x86_64-unknown-linux-gnu" ;;
  linux/arm64) TARGET_TRIPLE="aarch64-unknown-linux-gnu" ;;
  darwin/amd64) TARGET_TRIPLE="x86_64-apple-darwin" ;;
  darwin/arm64) TARGET_TRIPLE="aarch64-apple-darwin" ;;
  *)
    echo "error: unsupported os/arch: $OS/$ARCH" >&2
    exit 1
    ;;
esac

FORGE_VERSION="$VERSION"
if [[ "$FORGE_VERSION" != v* ]]; then
  FORGE_VERSION="v$FORGE_VERSION"
fi
VERSION_NO_V="${FORGE_VERSION#v}"

FORGE_COMMIT="$(git rev-parse --short HEAD 2>/dev/null || echo "none")"
FORGE_BUILD_DATE="$(date -u +"%Y-%m-%dT%H:%M:%SZ")"

export FORGE_VERSION FORGE_COMMIT FORGE_BUILD_DATE

rustup target add "$TARGET_TRIPLE" >/dev/null

cargo build --locked --release --target "$TARGET_TRIPLE" -p forge-cli --bin rforge
cargo build --locked --release --target "$TARGET_TRIPLE" -p forge-daemon --bin rforged
cargo build --locked --release --target "$TARGET_TRIPLE" -p forge-runner --bin forge-agent-runner
cargo build --locked --release --target "$TARGET_TRIPLE" -p fmail-cli --bin rfmail

BIN_DIR="target/$TARGET_TRIPLE/release"
STAGE_DIR="$(mktemp -d)"
trap 'rm -rf "$STAGE_DIR"' EXIT

cp "$BIN_DIR/rforge" "$STAGE_DIR/forge"
cp "$BIN_DIR/rforged" "$STAGE_DIR/forged"
cp "$BIN_DIR/forge-agent-runner" "$STAGE_DIR/forge-agent-runner"
cp "$BIN_DIR/rfmail" "$STAGE_DIR/fmail"

chmod 0755 "$STAGE_DIR/forge" "$STAGE_DIR/forged" "$STAGE_DIR/forge-agent-runner" "$STAGE_DIR/fmail"

mkdir -p "$OUT_DIR"
ARCHIVE_PATH="$OUT_DIR/forge_${VERSION_NO_V}_${OS}_${ARCH}.tar.gz"
tar -C "$STAGE_DIR" -czf "$ARCHIVE_PATH" forge forged forge-agent-runner fmail

echo "built $ARCHIVE_PATH"

#!/usr/bin/env sh
set -eu

REPO="JohnKesko/bfiles"
BIN_NAME="bfiles"
INSTALL_DIR="${BFILES_INSTALL_DIR:-$HOME/.local/bin}"

OS="$(uname -s)"
ARCH="$(uname -m)"

case "$OS" in
  Linux)
    case "$ARCH" in
      x86_64|amd64)
        ASSET="bfiles-linux-x86_64.tar.gz"
        ;;
      *)
        echo "Unsupported Linux architecture: $ARCH" >&2
        exit 1
        ;;
    esac
    ;;
  Darwin)
    case "$ARCH" in
      arm64|aarch64)
        ASSET="bfiles-macos-arm64.tar.gz"
        ;;
      x86_64|amd64)
        ASSET="bfiles-macos-x86_64.tar.gz"
        ;;
      *)
        echo "Unsupported macOS architecture: $ARCH" >&2
        exit 1
        ;;
    esac
    ;;
  *)
    echo "Unsupported OS: $OS" >&2
    exit 1
    ;;
esac

URL="https://github.com/$REPO/releases/latest/download/$ASSET"

TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

echo "Downloading $URL"
curl -fsSL "$URL" -o "$TMP_DIR/$ASSET"

tar -xzf "$TMP_DIR/$ASSET" -C "$TMP_DIR"

BIN_PATH="$(find "$TMP_DIR" -type f -name "$BIN_NAME" | head -n 1)"

if [ -z "$BIN_PATH" ]; then
  echo "Could not find $BIN_NAME in archive" >&2
  exit 1
fi

mkdir -p "$INSTALL_DIR"
cp "$BIN_PATH" "$INSTALL_DIR/$BIN_NAME"
chmod +x "$INSTALL_DIR/$BIN_NAME"

echo "Installed $BIN_NAME to $INSTALL_DIR/$BIN_NAME"

case ":$PATH:" in
  *":$INSTALL_DIR:"*)
    ;;
  *)
    echo ""
    echo "Add this to your shell profile if bfiles is not found:"
    echo "export PATH=\"\$HOME/.local/bin:\$PATH\""
    ;;
esac
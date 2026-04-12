#!/usr/bin/env bash
set -euo pipefail

REPO="parisgroup/graphify"

OS=$(uname -s | tr '[:upper:]' '[:lower:]')
ARCH=$(uname -m)

case "$OS" in
  darwin) OS="apple-darwin" ;;
  linux)  OS="unknown-linux-musl" ;;
  *)      echo "Unsupported OS: $OS"; exit 1 ;;
esac

case "$ARCH" in
  x86_64)  ARCH="x86_64" ;;
  aarch64|arm64) ARCH="aarch64" ;;
  *)       echo "Unsupported architecture: $ARCH"; exit 1 ;;
esac

TARGET="${ARCH}-${OS}"
echo "Detected target: ${TARGET}"

LATEST=$(curl -sL "https://api.github.com/repos/${REPO}/releases/latest" | grep '"tag_name"' | sed -E 's/.*"([^"]+)".*/\1/')
if [ -z "$LATEST" ]; then
  echo "Could not determine latest release"; exit 1
fi
echo "Latest version: ${LATEST}"

URL="https://github.com/${REPO}/releases/download/${LATEST}/graphify-${TARGET}.tar.gz"
echo "Downloading from: ${URL}"
TMPDIR=$(mktemp -d)
curl -sL "$URL" -o "${TMPDIR}/graphify.tar.gz"
tar xzf "${TMPDIR}/graphify.tar.gz" -C "${TMPDIR}"

INSTALL_DIR="/usr/local/bin"
if [ -w "$INSTALL_DIR" ]; then
  mv "${TMPDIR}/graphify" "${INSTALL_DIR}/graphify"
else
  sudo mv "${TMPDIR}/graphify" "${INSTALL_DIR}/graphify"
fi

rm -rf "$TMPDIR"
echo "graphify installed to ${INSTALL_DIR}/graphify"
graphify --version

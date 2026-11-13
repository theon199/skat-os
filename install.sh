#!/bin/sh
# skat-os single-command installer
# Usage:
#   curl -sSf https://raw.githubusercontent.com/theon199/skat-os/master/install.sh | sh
#
# Environment variables:
#   SKAT_VERSION: Release version to install (default: latest, e.g. v0.1.0)
#   SKAT_INSTALL_DIR: Target directory for installation (default: /usr/local/bin if writable, else ~/.local/bin)

set -e

REPO="theon199/skat-os"
BINARY_NAME="skat-os"

# Setup terminal color formatting if running interactively
if [ -t 1 ]; then
  BOLD="\033[1m"
  GREEN="\033[32m"
  BLUE="\033[34m"
  YELLOW="\033[33m"
  RED="\033[31m"
  RESET="\033[0m"
else
  BOLD=""
  GREEN=""
  BLUE=""
  YELLOW=""
  RED=""
  RESET=""
fi

info() {
  printf "${BLUE}${BOLD}==>${RESET} %s\n" "$1"
}

success() {
  printf "${GREEN}${BOLD}==>${RESET} ${GREEN}%s${RESET}\n" "$1"
}

warn() {
  printf "${YELLOW}${BOLD}Warning:${RESET} %s\n" "$1"
}

error() {
  printf "${RED}${BOLD}Error:${RESET} %s\n" "$1" >&2
  exit 1
}

# Determine Operating System
detect_os() {
  OS="$(uname -s)"
  case "$OS" in
    Linux)
      echo "linux"
      ;;
    Darwin)
      echo "darwin"
      ;;
    CYGWIN*|MINGW*|MSYS*)
      error "Windows detected. Please use the precompiled zip archive from https://github.com/${REPO}/releases or run 'cargo install skat-os'."
      ;;
    *)
      error "Unsupported operating system: $OS"
      ;;
  esac
}

# Determine Architecture
detect_arch() {
  ARCH="$(uname -m)"
  case "$ARCH" in
    x86_64|amd64)
      echo "x86_64"
      ;;
    aarch64|arm64|armv8*)
      echo "aarch64"
      ;;
    *)
      error "Unsupported architecture: $ARCH"
      ;;
  esac
}

# Detect downloader (curl or wget)
downloader() {
  if command -v curl >/dev/null 2>&1; then
    curl -sSfL "$1" -o "$2"
  elif command -v wget >/dev/null 2>&1; then
    wget -qO "$2" "$1"
  else
    error "Neither curl nor wget was found. Please install curl or wget to continue."
  fi
}

# Fetch latest release version from GitHub API
get_latest_version() {
  if command -v curl >/dev/null 2>&1; then
    LATEST=$(curl -sSf "https://api.github.com/repos/${REPO}/releases/latest" 2>/dev/null | grep '"tag_name":' | sed -E 's/.*"([^"]+)".*/\1/' || true)
  elif command -v wget >/dev/null 2>&1; then
    LATEST=$(wget -qO- "https://api.github.com/repos/${REPO}/releases/latest" 2>/dev/null | grep '"tag_name":' | sed -E 's/.*"([^"]+)".*/\1/' || true)
  fi

  if [ -z "$LATEST" ]; then
    # Fallback to default version if API is rate-limited or unavailable
    LATEST="v0.1.0"
  fi
  echo "$LATEST"
}

# Compute SHA-256 checksum of a file
compute_sha256() {
  FILE="$1"
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$FILE" | awk '{print $1}'
  elif command -v shasum >/dev/null 2>&1; then
    shasum -a 256 "$FILE" | awk '{print $1}'
  elif command -v openssl >/dev/null 2>&1; then
    openssl dgst -sha256 "$FILE" | awk '{print $NF}'
  else
    echo ""
  fi
}

main() {
  printf "\n"
  printf "${BOLD}%s${RESET}\n" "⚡ skat-os — Terminal Document Operating System Installer"
  printf "\n"

  DETECTED_OS="$(detect_os)"
  DETECTED_ARCH="$(detect_arch)"

  # Map OS + ARCH to Rust target triple
  case "${DETECTED_OS}-${DETECTED_ARCH}" in
    linux-x86_64)
      TARGET="x86_64-unknown-linux-gnu"
      ;;
    linux-aarch64)
      TARGET="aarch64-unknown-linux-gnu"
      ;;
    darwin-x86_64)
      TARGET="x86_64-apple-darwin"
      ;;
    darwin-aarch64)
      TARGET="aarch64-apple-darwin"
      ;;
    *)
      error "No prebuilt binaries available for ${DETECTED_OS}-${DETECTED_ARCH}."
      ;;
  esac

  # Determine version
  VERSION="${SKAT_VERSION:-${VERSION:-}}"
  if [ -z "$VERSION" ] || [ "$VERSION" = "latest" ]; then
    info "Finding latest release..."
    VERSION="$(get_latest_version)"
  fi

  # Ensure version has leading 'v'
  case "$VERSION" in
    v*) ;;
    *) VERSION="v$VERSION" ;;
  esac

  info "Selected version: ${BOLD}${VERSION}${RESET}"
  info "Detected platform: ${BOLD}${TARGET}${RESET}"

  # Determine installation directory
  if [ -n "$SKAT_INSTALL_DIR" ]; then
    INSTALL_DIR="$SKAT_INSTALL_DIR"
  elif [ -n "$INSTALL_DIR" ]; then
    INSTALL_DIR="$INSTALL_DIR"
  elif [ -w "/usr/local/bin" ]; then
    INSTALL_DIR="/usr/local/bin"
  else
    INSTALL_DIR="${HOME}/.local/bin"
  fi

  info "Installing to: ${BOLD}${INSTALL_DIR}${RESET}"

  # Create temporary working directory
  TMP_DIR="$(mktemp -d 2>/dev/null || mktemp -d -t 'skat-os')"
  cleanup() {
    rm -rf "$TMP_DIR"
  }
  trap cleanup EXIT INT TERM

  ARCHIVE_NAME="skat-os-${VERSION}-${TARGET}.tar.gz"
  CHECKSUM_NAME="${ARCHIVE_NAME}.sha256"
  DOWNLOAD_URL="https://github.com/${REPO}/releases/download/${VERSION}/${ARCHIVE_NAME}"
  CHECKSUM_URL="https://github.com/${REPO}/releases/download/${VERSION}/${CHECKSUM_NAME}"

  ARCHIVE_PATH="${TMP_DIR}/${ARCHIVE_NAME}"
  CHECKSUM_PATH="${TMP_DIR}/${CHECKSUM_NAME}"

  info "Downloading ${ARCHIVE_NAME}..."
  if ! downloader "$DOWNLOAD_URL" "$ARCHIVE_PATH"; then
    error "Failed to download release archive from ${DOWNLOAD_URL}. Check if the version and target exist."
  fi

  # Download and verify checksum if available
  info "Verifying SHA-256 checksum..."
  if downloader "$CHECKSUM_URL" "$CHECKSUM_PATH" 2>/dev/null; then
    EXPECTED_HASH="$(awk '{print $1}' "$CHECKSUM_PATH" | tr '[:upper:]' '[:lower:]')"
    ACTUAL_HASH="$(compute_sha256 "$ARCHIVE_PATH" | tr '[:upper:]' '[:lower:]')"

    if [ -n "$ACTUAL_HASH" ]; then
      if [ "$EXPECTED_HASH" != "$ACTUAL_HASH" ]; then
        error "Checksum verification failed!\nExpected: $EXPECTED_HASH\nActual:   $ACTUAL_HASH"
      fi
      info "Checksum verified successfully (${ACTUAL_HASH})"
    else
      warn "No sha256 tool found; skipped checksum verification."
    fi
  else
    warn "Checksum file not found on release; skipping verification."
  fi

  # Extract binary
  info "Extracting ${BINARY_NAME}..."
  tar -xzf "$ARCHIVE_PATH" -C "$TMP_DIR"

  if [ ! -f "${TMP_DIR}/${BINARY_NAME}" ]; then
    error "Extraction failed: binary '${BINARY_NAME}' not found inside archive."
  fi

  # Ensure destination directory exists
  mkdir -p "$INSTALL_DIR"

  # Move binary
  mv "${TMP_DIR}/${BINARY_NAME}" "${INSTALL_DIR}/${BINARY_NAME}"
  chmod +x "${INSTALL_DIR}/${BINARY_NAME}"

  success "skat-os ${VERSION} installed successfully to ${INSTALL_DIR}/${BINARY_NAME}!"

  # Check PATH
  case ":${PATH}:" in
    *":${INSTALL_DIR}:"*)
      ;;
    *)
      printf "\n"
      warn "${INSTALL_DIR} is not in your PATH."
      printf "To run 'skat-os' from anywhere, add the following line to your shell configuration profile (~/.bashrc, ~/.zshrc, or ~/.profile):\n\n"
      printf "    export PATH=\"%s:\$PATH\"\n\n" "$INSTALL_DIR"
      ;;
  esac

  printf "\n"
  printf "Get started:\n"
  printf "    %s\n\n" "${BINARY_NAME} --help"
}

main "$@"

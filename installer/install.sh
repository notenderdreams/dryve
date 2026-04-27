#!/usr/bin/env bash
set -euo pipefail

REPO="notenderdreams/dryve"
BINARY="dryve"
INSTALL_DIR="${INSTALL_DIR:-/usr/local/bin}"

# --- Helpers ---

info()  { printf '\033[1;34m[info]\033[0m  %s\n' "$*"; }
error() { printf '\033[1;31m[error]\033[0m %s\n' "$*" >&2; exit 1; }

need() {
  command -v "$1" > /dev/null 2>&1 || error "Required command '$1' not found."
}

# --- Detect platform ---

detect_platform() {
  local os arch

  os="$(uname -s)"
  arch="$(uname -m)"

  case "$os" in
    Linux*)  os="linux" ;;
    Darwin*) os="macos" ;;
    *)       error "Unsupported OS: $os" ;;
  esac

  case "$arch" in
    x86_64)       arch="x86_64" ;;
    aarch64|arm64) arch="arm64"  ;;
    *)            error "Unsupported architecture: $arch" ;;
  esac

  echo "${os}" "${arch}"
}

# --- Fetch latest tag ---

get_latest_version() {
  need curl
  local url="https://api.github.com/repos/${REPO}/releases/latest"
  curl -fsSL "$url" | grep -o '"tag_name": *"[^"]*"' | head -1 | cut -d'"' -f4
}

# --- Main ---

main() {
  need curl
  need tar

  read -r os arch <<< "$(detect_platform)"
  info "Detected platform: ${os}-${arch}"

  local version="${1:-}"
  if [ -z "$version" ]; then
    info "Fetching latest release..."
    version="$(get_latest_version)"
  fi
  [ -z "$version" ] && error "Could not determine latest version."
  info "Version: ${version}"

  local asset="${BINARY}-${version}-${os}-${arch}.tar.gz"
  local url="https://github.com/${REPO}/releases/download/${version}/${asset}"

  info "Downloading ${url}..."
  local tmp
  tmp="$(mktemp -d)"
  curl -fsSL "$url" -o "${tmp}/${asset}" || error "Download failed. Check that the release exists."

  info "Extracting..."
  tar xzf "${tmp}/${asset}" -C "${tmp}"

  info "Installing to ${INSTALL_DIR}..."
  if [ -w "$INSTALL_DIR" ]; then
    mv "${tmp}/${BINARY}" "${INSTALL_DIR}/${BINARY}"
  else
    sudo mv "${tmp}/${BINARY}" "${INSTALL_DIR}/${BINARY}"
  fi
  chmod +x "${INSTALL_DIR}/${BINARY}"

  rm -rf "$tmp"

  info "dryve ${version} installed to ${INSTALL_DIR}/${BINARY}"
  info "Run 'dryve --help' to get started."
}

main "$@"

#!/bin/sh
# zforge installer
# Usage: curl --proto '=https' --tlsv1.2 -sSf https://raw.githubusercontent.com/zanep298/zforge/main/install.sh | sh
#
# ZFORGE_VERSION  release tag to install (default: the latest), e.g. v1.5.0
# ZFORGE_INSTALL  directory to install into (default: ~/.local/bin)
# (ZF_VERSION / ZF_INSTALL are still read when the ZFORGE_ ones are unset.)
#
# Release binaries: Linux x86_64, macOS x86_64 and arm64.

set -e

REPO="zanep298/zforge"
BINARY="zforge"

# Detect OS and architecture
detect_platform() {
    _os="$(uname -s)"
    _arch="$(uname -m)"

    case "$_os" in
        Linux)
            case "$_arch" in
                x86_64) echo "x86_64-unknown-linux-gnu" ;;
                *) echo "unsupported: $_arch on Linux (release binaries: x86_64); build from source: cargo install --git https://github.com/$REPO" >&2; exit 1 ;;
            esac
            ;;
        Darwin)
            case "$_arch" in
                x86_64) echo "x86_64-apple-darwin" ;;
                arm64) echo "aarch64-apple-darwin" ;;
                *) echo "unsupported: $_arch on macOS" >&2; exit 1 ;;
            esac
            ;;
        *)
            echo "unsupported OS: $_os" >&2; exit 1 ;;
    esac
}

get_latest_version() {
    curl --proto '=https' --tlsv1.2 -sSf \
        "https://api.github.com/repos/$REPO/releases/latest" |
        grep '"tag_name":' | sed -E 's/.*"([^"]+)".*/\1/'
}

main() {
    platform="$(detect_platform)"
    version="${ZFORGE_VERSION:-${ZF_VERSION:-}}"
    if [ -z "$version" ]; then
        version="$(get_latest_version)"
    fi

    if [ -z "$version" ]; then
        echo "error: could not determine latest version" >&2
        exit 1
    fi

    echo "Installing $BINARY $version for $platform..."

    url="https://github.com/$REPO/releases/download/$version/${BINARY}-${platform}.tar.gz"
    tmpdir="$(mktemp -d)"
    trap 'rm -rf "$tmpdir"' EXIT

    curl --proto '=https' --tlsv1.2 -sSfL "$url" -o "$tmpdir/${BINARY}.tar.gz"
    tar xzf "$tmpdir/${BINARY}.tar.gz" -C "$tmpdir"

    install_dir="${ZFORGE_INSTALL:-${ZF_INSTALL:-$HOME/.local/bin}}"
    mkdir -p "$install_dir"
    cp "$tmpdir/$BINARY" "$install_dir/"
    chmod +x "$install_dir/$BINARY"

    echo "$BINARY installed to $install_dir/$BINARY"

    if ! echo "$PATH" | grep -q "$install_dir"; then
        echo ""
        echo "⚠️  $install_dir is not in your PATH."
        echo "   Add this to your shell profile:"
        echo "       export PATH=\"$install_dir:\$PATH\""
    fi

    echo ""
    echo "Run '$BINARY --version' to verify."
}

main

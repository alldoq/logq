#!/usr/bin/env bash
# Render packaging/homebrew/logq.rb.tmpl using SHA256 sums from a published
# GitHub release. Output is printed to stdout; pipe it into your tap.
#
# Usage:
#   scripts/update-homebrew.sh v0.1.2 > Formula/logq.rb
set -euo pipefail

TAG="${1:?usage: update-homebrew.sh <tag>}"
VERSION="${TAG#v}"
REPO="alldoq/logq"
TMPL="$(dirname "$0")/../packaging/homebrew/logq.rb.tmpl"

fetch_sha() {
  local target="$1"
  local url="https://github.com/${REPO}/releases/download/${TAG}/logq-${TAG}-${target}.tar.gz.sha256"
  curl -fsSL "$url" | awk '{print $1}'
}

SHA_DARWIN_ARM=$(fetch_sha aarch64-apple-darwin)
SHA_DARWIN_X86=$(fetch_sha x86_64-apple-darwin)
SHA_LINUX_ARM=$(fetch_sha aarch64-unknown-linux-gnu)
SHA_LINUX_X86=$(fetch_sha x86_64-unknown-linux-gnu)

sed \
  -e "s|__VERSION__|${VERSION}|g" \
  -e "s|__SHA_DARWIN_ARM__|${SHA_DARWIN_ARM}|g" \
  -e "s|__SHA_DARWIN_X86__|${SHA_DARWIN_X86}|g" \
  -e "s|__SHA_LINUX_ARM__|${SHA_LINUX_ARM}|g" \
  -e "s|__SHA_LINUX_X86__|${SHA_LINUX_X86}|g" \
  "$TMPL"

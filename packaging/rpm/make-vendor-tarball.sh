#!/usr/bin/env bash
# Builds the two sources sam.spec compiles from, out of the committed tree:
#
#   packaging/rpm/make-vendor-tarball.sh [output-dir]   (default ~/rpmbuild/SOURCES)
#
#   sam-<version>.tar.gz           git archive of HEAD
#   sam-<version>-vendor.tar.xz    every locked crate, plus the source
#                                  replacement cargo needs to use them offline
set -euo pipefail

cd "$(dirname "$0")/../.."
version="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)"
out="${1:-$HOME/rpmbuild/SOURCES}"
mkdir -p "$out"

if ! git diff --quiet HEAD --; then
  echo "warning: uncommitted changes are not in the source tarball" >&2
fi
git archive --format=tar.gz --prefix="sam-${version}/" -o "$out/sam-${version}.tar.gz" HEAD

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
cargo vendor --locked --versioned-dirs "$work/vendor" > "$work/full-config.toml"
# The directory must be relative, so the tarball works wherever it is unpacked.
sed 's|^directory = .*|directory = "vendor"|' "$work/full-config.toml" > "$work/vendor-config.toml"
tar -cJf "$out/sam-${version}-vendor.tar.xz" -C "$work" vendor vendor-config.toml

echo "$out/sam-${version}.tar.gz"
echo "$out/sam-${version}-vendor.tar.xz"

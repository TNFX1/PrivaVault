#!/bin/sh
# Prepare the AUR package for a published release: set pkgver, pin the source checksum, write .SRCINFO.
# Usage: tools/release/aur.sh 2.2.1    (then copy PKGBUILD + .SRCINFO into the AUR git repo and push)
set -eu
v="${1:?version, e.g. 2.2.1}"
dir="$(dirname "$0")/../../packaging/aur/privavault"
tmp="$(mktemp)"
trap 'rm -f "$tmp"' EXIT
curl -fsSL --retry 8 --retry-all-errors --retry-delay 10 -o "$tmp" \
  "https://github.com/TNFX1/PrivaVault/archive/refs/tags/v$v.tar.gz"
sum="$(sha256sum "$tmp" | cut -d' ' -f1)"
sed -i -e "s/^pkgver=.*/pkgver=$v/" -e "s/^pkgrel=.*/pkgrel=1/" \
  -e "s/^sha256sums=.*/sha256sums=('$sum')/" "$dir/PKGBUILD"
(cd "$dir" && makepkg --printsrcinfo > .SRCINFO)
echo "AUR package ready for $v ($sum) in $dir"

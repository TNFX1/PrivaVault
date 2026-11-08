#!/bin/sh
# Build the Flatpak from this checkout and install it for the current user.
# Needs: flatpak install --user flathub org.flatpak.Builder
# Run:   tools/release/flatpak.sh   then   flatpak run io.github.tnfx1.PrivaVault
set -eu
cd "$(dirname "$0")/../.."
python3 tools/release/flatpak_cargo_sources.py
flatpak run org.flatpak.Builder --user --install --force-clean --install-deps-from=flathub \
  --state-dir=target/flatpak/state target/flatpak/build packaging/flatpak/io.github.tnfx1.PrivaVault.yml

#!/usr/bin/env python3
"""Build latest.json, the update manifest read by the in-app updater (src-tauri/src/update.rs).

Usage: make_manifest.py <version> <dir with packages and .sig files> <notes.md> <owner/repo> > latest.json

Every package must have a <package>.sig produced by `tauri signer sign --app-version <version>`.
Fails if any expected package or signature is missing, so a release can never ship a manifest
that points some users at nothing (or at an unsigned file).
"""
import datetime
import json
import sys
from pathlib import Path

# manifest key -> how to recognise the package file
TARGETS = {
    "windows-x86_64-nsis": lambda n: n.endswith("_x64-setup.exe"),
    "linux-x86_64-appimage": lambda n: n.endswith("_amd64.AppImage"),
    "linux-x86_64-deb": lambda n: n.endswith("_amd64.deb"),
    "linux-x86_64-rpm": lambda n: n.endswith(".x86_64.rpm"),
    "linux-x86_64-pacman": lambda n: n.endswith("-x86_64.pkg.tar.zst") and "-debug-" not in n,
}


def main() -> None:
    version, folder, notes_file, repo = sys.argv[1:5]
    folder = Path(folder)
    files = sorted(p.name for p in folder.iterdir() if p.is_file() and not p.name.endswith(".sig"))
    platforms = {}
    for key, matches in TARGETS.items():
        found = [n for n in files if matches(n)]
        if len(found) != 1:
            sys.exit(f"{key}: expected exactly one package, found {found}")
        name = found[0]
        sig = folder / f"{name}.sig"
        if not sig.is_file():
            sys.exit(f"{key}: missing signature {sig.name}")
        platforms[key] = {
            "signature": sig.read_text().strip(),
            "url": f"https://github.com/{repo}/releases/download/v{version}/{name}",
        }
    manifest = {
        "version": version,
        "notes": Path(notes_file).read_text().strip(),
        "pub_date": datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
        "platforms": platforms,
    }
    json.dump(manifest, sys.stdout, indent=2)
    sys.stdout.write("\n")


if __name__ == "__main__":
    main()

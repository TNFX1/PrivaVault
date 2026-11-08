#!/usr/bin/env python3
"""Write packaging/flatpak/cargo-sources.json from Cargo.lock (crates.io dependencies only).

Each crate becomes a flatpak-builder archive source unpacked into cargo/vendor/<name>-<version>, plus
the .cargo-checksum.json cargo expects in a vendored crate, and a cargo config that points crates.io at
that vendor directory. The build then runs with `cargo --offline`. Run after every Cargo.lock change.
"""
import json
import sys
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
CRATES_IO = "registry+https://github.com/rust-lang/crates.io-index"


def main():
    lock = tomllib.loads((ROOT / "Cargo.lock").read_text())
    sources = []
    for pkg in lock["package"]:
        src = pkg.get("source")
        if src is None:
            continue  # workspace member
        if src != CRATES_IO:
            sys.exit(f"unsupported source for {pkg['name']}: {src}")
        name, version, checksum = pkg["name"], pkg["version"], pkg["checksum"]
        dest = f"cargo/vendor/{name}-{version}"
        sources.append({
            "type": "archive",
            "archive-type": "tar-gzip",
            "url": f"https://static.crates.io/crates/{name}/{name}-{version}.crate",
            "sha256": checksum,
            "dest": dest,
        })
        sources.append({
            "type": "inline",
            "contents": json.dumps({"package": checksum, "files": {}}),
            "dest": dest,
            "dest-filename": ".cargo-checksum.json",
        })
    sources.append({
        "type": "inline",
        "contents": '[source.crates-io]\nreplace-with = "vendored-sources"\n\n'
                    '[source.vendored-sources]\ndirectory = "cargo/vendor"\n',
        "dest": "cargo",
        "dest-filename": "config.toml",
    })
    out = ROOT / "packaging/flatpak/cargo-sources.json"
    out.write_text(json.dumps(sources, indent=1) + "\n")
    print(f"{out.relative_to(ROOT)}: {len(sources) // 2} crates")


if __name__ == "__main__":
    main()

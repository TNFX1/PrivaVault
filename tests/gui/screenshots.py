#!/usr/bin/env python3
"""Regenerate the README screenshots from the real app (debug build), light and dark.

Usage:  cargo build -p privavault && python3 tests/gui/screenshots.py
Writes docs/screenshots/{lock,vault}-{light,dark}-<version>.png and assets/preview-<version>.png and
points README.md at them. Only the app's own page
is captured (WebKit inspector), never the desktop. Sample files are random bytes in a temp folder.
"""
import json
import re
import os
import shutil
import sys
import tempfile
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
from e2e import BIN, ROOT, App, shot  # noqa: E402

PW = "correct horse battery staple ocean"
SHOTS = ROOT / "docs" / "screenshots"
VERSION = re.search(r'^version = "(.+)"', (ROOT / "Cargo.toml").read_text(), re.M).group(1)


def theme(app, name, back_to):
    app.js('document.querySelector(\'[data-view="settings"]\').click(); return 1;')
    app.js(f'document.querySelector(\'input[name=theme][value={name}]\').click(); return 1;')
    app.wait(f'document.documentElement.dataset.theme === "{name}"', what=f"{name} theme")
    app.js(f'document.querySelector(\'[data-view="{back_to}"]\').click(); return 1;')
    time.sleep(0.6)  # let the view transition finish


def main():
    if not BIN.exists():
        sys.exit("build first: cargo build -p privavault")
    work = Path(tempfile.mkdtemp(prefix="pv-shots-", dir=ROOT / "target"))
    (work / "out").mkdir()
    (work / "config/privavault").mkdir(parents=True)
    (work / "config/privavault/settings.json").write_text(json.dumps({"onboarding_done": True, "theme": "light"}))
    d = work / "data"
    files = {
        "Tax 2025/receipts.zip": 2_300_000, "Tax 2025/return-2025.pdf": 178_000,
        "Photos/Istanbul trip/IMG_2041.heic": 3_000_000, "Photos/Istanbul trip/IMG_2042.heic": 2_800_000,
        "passport-scan.pdf": 625_000, "recovery-codes.txt": 1_200,
    }
    for rel, size in files.items():
        (d / rel).parent.mkdir(parents=True, exist_ok=True)
        (d / rel).write_bytes(os.urandom(size))

    app = App(work, [d / "Tax 2025", d / "Photos", d / "passport-scan.pdf", d / "recovery-codes.txt"])
    tmp = work / "shots"
    tmp.mkdir()
    try:
        app.wait('document.querySelectorAll("#selList .name").length === 4', what="selection")
        app.type("encPw", PW)
        app.type("encPw2", PW)
        shot(app, tmp, "lock-light")
        theme(app, "dark", "encrypt")
        shot(app, tmp, "lock-dark")

        # The save dialog would suggest "vault-<date>"; name it like a user would.
        app.js('suggestedName = () => "Personal documents"; return 1;')
        app.click("btnEncrypt")
        app.wait('!document.getElementById("encResult").hidden', 120, "locked")
        app.second_instance(next((work / "out").glob("*.pvault")))
        app.wait('!document.getElementById("pickedVault").hidden', what="vault picked")
        app.type("openPw", PW)
        app.click("btnUnlock")
        app.wait('!document.getElementById("vaultPane").hidden', 60, "vault open")
        shot(app, tmp, "vault-dark")
        theme(app, "light", "open")
        shot(app, tmp, "vault-light")

        # File names carry the version so GitHub and browsers never show a cached older picture.
        for old in [*SHOTS.glob("*.png"), *ROOT.glob("assets/preview*.png")]:
            old.unlink()
        for name in ("lock-light", "lock-dark", "vault-light", "vault-dark"):
            shutil.copy(tmp / f"{name}.png", SHOTS / f"{name}-{VERSION}.png")
        shutil.copy(tmp / "vault-dark.png", ROOT / "assets" / f"preview-{VERSION}.png")
        readme = ROOT / "README.md"
        readme.write_text(re.sub(r"((?:docs/screenshots/[a-z]+-[a-z]+|assets/preview))(?:-[0-9.]+)?\.png",
                                 rf"\1-{VERSION}.png", readme.read_text()))
        print("wrote", ", ".join(sorted(p.name for p in tmp.iterdir())))
    finally:
        app.close()
        shutil.rmtree(work, ignore_errors=True)


if __name__ == "__main__":
    main()

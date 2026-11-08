#!/usr/bin/env python3
"""End-to-end test of one-click updates (Linux, Arch/pacman path), without touching the system.

Builds nothing itself. Run after:  PRIVAVAULT_PACKAGE=pacman cargo build -p privavault
It creates a throwaway signing key, serves a fake "9.9.9" release over localhost, replaces
`pkexec` with a recorder on PATH, then drives the real app window:

1. "Check now" finds 9.9.9 and offers "Update now".
2. A package whose bytes were altered after signing is rejected *before* pkexec runs.
3. A correctly signed package is downloaded, verified and handed to `pkexec pacman -U`.
"""
import http.server
import json
import os
import shutil
import subprocess
import sys
import tempfile
import threading
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
from inspector import Inspector  # noqa: E402

ROOT = Path(__file__).resolve().parents[2]
BIN = ROOT / "target/debug/privavault"
TAURI = ROOT / ".tools/bin/cargo-tauri"
PORT, IPORT = 8765, 9241
results = []


def check(name, cond, detail=""):
    results.append(bool(cond))
    print(f"{'PASS' if cond else 'FAIL'}  {name}{('  — ' + str(detail)) if detail and not cond else ''}")


def run_app(work, env_extra):
    env = dict(os.environ, XDG_CONFIG_HOME=str(work / "config"), WEBKIT_INSPECTOR_HTTP_SERVER=f"127.0.0.1:{IPORT}",
               PATH=f"{work / 'bin'}:{os.environ['PATH']}", **env_extra)
    p = subprocess.Popen([str(BIN)], env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    i = Inspector(IPORT)
    i.wait('typeof state !== "undefined" && state.info', what="ui")
    return p, i


def main():
    if not BIN.exists() or not TAURI.exists():
        sys.exit("needs target/debug/privavault (built with PRIVAVAULT_PACKAGE=pacman) and .tools/bin/cargo-tauri")
    work = Path(tempfile.mkdtemp(prefix="pv-update-", dir=ROOT / "target"))
    srv, binp = work / "srv", work / "bin"
    srv.mkdir(); binp.mkdir()
    (work / "config/privavault").mkdir(parents=True)
    (work / "config/privavault/settings.json").write_text('{"onboarding_done": true}')
    log = work / "pkexec.log"
    (binp / "pkexec").write_text(f'#!/bin/sh\necho "$@" >> "{log}"\nexit 0\n')
    (binp / "pkexec").chmod(0o755)

    pw = "test-password"
    subprocess.run([str(TAURI), "signer", "generate", "--ci", "-p", pw, "-w", str(work / "k.key")],
                   check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    pubkey = (work / "k.key.pub").read_text().strip()
    pkg = srv / "privavault-9.9.9-1-x86_64.pkg.tar.zst"
    pkg.write_bytes(os.urandom(200_000))
    subprocess.run([str(TAURI), "signer", "sign", "-f", str(work / "k.key"), "-p", pw, "--app-version", "9.9.9", str(pkg)],
                   check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    manifest = {"version": "9.9.9", "notes": "test", "pub_date": "2026-11-07T00:00:00Z", "platforms": {
        "linux-x86_64-pacman": {"signature": Path(f"{pkg}.sig").read_text().strip(),
                                "url": f"http://127.0.0.1:{PORT}/{pkg.name}"}}}
    (srv / "latest.json").write_text(json.dumps(manifest))

    class Handler(http.server.SimpleHTTPRequestHandler):
        def __init__(self, *a, **k):
            super().__init__(*a, directory=str(srv), **k)

        def log_message(self, *a):
            pass

    httpd = http.server.ThreadingHTTPServer(("127.0.0.1", PORT), Handler)
    threading.Thread(target=httpd.serve_forever, daemon=True).start()
    env = {"PRIVAVAULT_E2E_UPDATE_ENDPOINT": f"http://127.0.0.1:{PORT}/latest.json", "PRIVAVAULT_E2E_UPDATE_PUBKEY": pubkey}

    try:
        # --- tampered package: must be refused before anything privileged runs
        good = pkg.read_bytes()
        bad = bytearray(good); bad[1000] ^= 1
        pkg.write_bytes(bytes(bad))
        p, i = run_app(work, env)
        i.js('document.querySelector(\'[data-view="settings"]\').click(); document.getElementById("btnCheckNow").click(); return 1;')
        i.wait('!document.getElementById("updateBanner").hidden', what="banner")
        check("update 9.9.9 is offered", "9.9.9" in i.js('return document.getElementById("updateText").textContent;'))
        check("one-click button shown for pacman installs", i.js('return !document.getElementById("updateInstall").hidden;'))
        i.js('document.getElementById("updateInstall").click(); return 1;')
        time.sleep(0.5)
        i.wait('document.getElementById("busy").hidden', 60, "install attempt")
        msg = i.js('return document.getElementById("updateText").textContent;')
        check("tampered package rejected with a clear message", "couldn't be installed" in msg, msg)
        check("pkexec never ran for the tampered package", not log.exists())
        p.terminate(); p.wait(10)

        # --- genuine package: verified, then handed to pacman through pkexec
        pkg.write_bytes(good)
        p, i = run_app(work, env)
        i.js('document.getElementById("btnCheckNow").click(); return 1;')
        i.wait('!document.getElementById("updateBanner").hidden', what="banner")
        # Deferred click: a local install finishes in milliseconds and the app restarts itself,
        # which would close the inspector before a synchronous click() call returns.
        i.js('setTimeout(() => document.getElementById("updateInstall").click(), 50); return 1;')
        deadline = time.time() + 60
        while time.time() < deadline and not log.exists():
            time.sleep(0.3)
        line = log.read_text().strip() if log.exists() else ""
        check("signed package installed via 'pkexec pacman -U --noconfirm --needed <file>'",
              line.startswith("pacman -U --noconfirm --needed ") and line.endswith(".pkg.tar.zst"), line)
        time.sleep(2)
        tmp_dirs = list(Path(tempfile.gettempdir()).glob("privavault-update-*"))
        check("downloaded package removed afterwards", not tmp_dirs, tmp_dirs)
        p.terminate()
        subprocess.run(["pkill", "-x", "privavault"], check=False)  # the app restarts itself after updating
    finally:
        httpd.shutdown()
        shutil.rmtree(work, ignore_errors=True)
    print(f"\n{sum(results)}/{len(results)} checks passed")
    sys.exit(0 if all(results) else 1)


if __name__ == "__main__":
    main()

#!/usr/bin/env python3
"""End-to-end GUI test for PrivaVault on a Linux desktop session (Wayland or X11).

Drives the *real* debug build: real WebKitGTK page, real Tauri IPC, real Rust core, real files.
User input is simulated with DOM events through the WebKit remote inspector; native file dialogs
are answered by the debug-only PRIVAVAULT_E2E_OUTPUT_DIR seam; "open with" is exercised by
launching a real second instance (single-instance forwarding).

Usage:  cargo build -p privavault && python3 tests/gui/e2e.py [--shots DIR]
"""
import argparse
import base64
import hashlib
import json
import os
import shutil
import subprocess
import sys
import tempfile
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
from inspector import Inspector  # noqa: E402

ROOT = Path(__file__).resolve().parents[2]
BIN = ROOT / "target" / "debug" / "privavault"
FIX = ROOT / "crates" / "privavault-core" / "tests" / "fixtures"
PORT = 9231

results = []


def check(name, cond, detail=""):
    results.append((name, bool(cond), detail))
    print(f"{'PASS' if cond else 'FAIL'}  {name}{('  — ' + str(detail)) if detail and not cond else ''}")


def sha(p):
    return hashlib.sha256(Path(p).read_bytes()).hexdigest()


class App:
    def __init__(self, work, args):
        self.work = work
        self.env = dict(os.environ, XDG_CONFIG_HOME=str(work / "config"),
                        PRIVAVAULT_E2E_OUTPUT_DIR=str(work / "out"),
                        WEBKIT_INSPECTOR_HTTP_SERVER=f"127.0.0.1:{PORT}")
        self.proc = subprocess.Popen([str(BIN), *map(str, args)], env=self.env,
                                     stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        self.i = Inspector(PORT)
        self.i.wait('document.readyState === "complete" && window.PVUtil && document.getElementById("encTitle").textContent', what="UI ready")

    def js(self, code):
        return self.i.js(code)

    def wait(self, expr, timeout=30, what=None):
        try:
            return self.i.wait(expr, timeout, what)
        except TimeoutError:
            print("  (on timeout) messages:", self.js('return ["encMsg","openMsg","vaultMsg","vaultResultText","busyPhase"].map(i => i + "=" + document.getElementById(i).textContent);'))
            raise

    def click(self, el_id):
        self.js(f'document.getElementById("{el_id}").click(); return true;')

    def run(self, el_id, what):
        """Click a button that starts a backend operation and wait until it has finished."""
        self.click(el_id)
        time.sleep(0.2)
        self.wait('document.getElementById("busy").hidden', 120, what)

    def type(self, el_id, value):
        self.js(f'const e = document.getElementById("{el_id}"); e.value = {json.dumps(value)};'
                f'e.dispatchEvent(new Event("input", {{bubbles: true}})); return true;')

    def text(self, el_id):
        return self.js(f'return document.getElementById("{el_id}").textContent;')

    def visible(self, el_id):
        return self.js(f'const e = document.getElementById("{el_id}"); return !!e && !e.hidden && e.offsetParent !== null;')

    def second_instance(self, *paths):
        """Launch again like a file manager would; the running window must receive the paths."""
        env = {k: v for k, v in self.env.items() if k != "WEBKIT_INSPECTOR_HTTP_SERVER"}
        p = subprocess.run([str(BIN), *map(str, paths)], env=env, timeout=20,
                           stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        return p.returncode

    def close(self):
        self.proc.terminate()
        try:
            self.proc.wait(10)
        except subprocess.TimeoutExpired:
            self.proc.kill()


def shot(app, shots, name):
    """Capture the page itself through the inspector — never the desktop or other windows."""
    if not shots:
        return
    time.sleep(0.5)
    w, h = app.js("return [document.documentElement.clientWidth, document.documentElement.clientHeight];")
    r = app.i.call("Page.snapshotRect", {"x": 0, "y": 0, "width": w, "height": h, "coordinateSystem": "Viewport"})
    url = r.get("result", {}).get("dataURL", "")
    if url.startswith("data:image/png;base64,"):
        (Path(shots) / f"{name}.png").write_bytes(base64.b64decode(url.split(",", 1)[1]))
    else:
        print("  (screenshot unavailable)", str(r)[:200])


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--shots", help="directory for screenshots (KDE spectacle)")
    ap.add_argument("--keep", action="store_true")
    a = ap.parse_args()
    if not BIN.exists():
        sys.exit("build first: cargo build -p privavault")

    work = Path(tempfile.mkdtemp(prefix="pv-e2e-", dir=ROOT / "target"))
    (work / "out").mkdir()
    data = work / "data"
    (data / "Vacation 2026" / "raw").mkdir(parents=True)
    (data / "Vacation 2026" / "IMG_0412.jpg").write_bytes(os.urandom(700_000))
    (data / "Vacation 2026" / "raw" / "IMG_0412.dng").write_bytes(os.urandom(150_001))
    (data / "passport.pdf").write_bytes(b"%PDF-1.7 passport scan")
    (data / "<i>notes.txt").write_text("html-looking name")
    big = data / "big.iso"
    with open(big, "wb") as f:
        f.truncate(1_500_000_000)  # sparse: cheap to create, real bytes to encrypt

    app = App(work, [data / "Vacation 2026", data / "passport.pdf", data / "<i>notes.txt"])
    try:
        # --- first run -------------------------------------------------------------------------
        check("welcome dialog shown on first run", app.wait('document.getElementById("onboarding").open', what="welcome"))
        shot(app, a.shots, "01-welcome")
        app.click("obDone")
        app.wait('!document.getElementById("onboarding").open', what="welcome closed")
        settings = json.loads((work / "config/privavault/settings.json").read_text())
        check("welcome dismissal persisted", settings.get("onboarding_done") is True)
        check("update check off by default", settings.get("check_updates") is False)
        mode = (work / "config/privavault/settings.json").stat().st_mode & 0o777
        check("settings file is 0600", mode == 0o600, oct(mode))
        check("network status says offline", "Offline" in app.text("netText"))

        # --- lock files (launch args = "open with") -----------------------------------------------
        items = app.js('return [...document.querySelectorAll("#selList .name")].map(e => e.textContent);')
        check("launch arguments populate the selection", items == ["Vacation 2026", "passport.pdf", "<i>notes.txt"], items)
        check("HTML-looking file name rendered as text", app.js('return document.querySelectorAll("#selList i").length === 0;'))
        app.type("encPw", "short")
        app.type("encPw2", "short")
        app.click("btnEncrypt")
        check("short password refused", "too short" in app.text("encMsg"), app.text("encMsg"))
        pw = "correct horse battery staple ocean"
        app.type("encPw", pw)
        app.type("encPw2", pw + "x")
        check("mismatch hint shown", "don't match" in app.text("pwMatch"))
        app.click("btnEncrypt")
        check("mismatch refused", "don't match" in app.text("encMsg"))
        app.type("encPw2", pw)
        check("strength meter shows Strong", app.text("pwMeterText") == "Strong", app.text("pwMeterText"))
        shot(app, a.shots, "02-lock-files")
        app.click("btnEncrypt")
        app.wait('!document.getElementById("encResult").hidden', what="encrypt result")
        vault = next((work / "out").glob("*.pvault"), None)
        check("vault written to chosen location", vault is not None)
        check("vault has v2 magic", vault and vault.read_bytes()[:8] == b"PRIVAVLT")
        check("vault is 0600", vault and (vault.stat().st_mode & 0o777) == 0o600)
        check("password fields cleared after success", app.js('return document.getElementById("encPw").value === "";'))
        check("no partial files left", not list((work / "out").glob(".*pvpart")))
        shot(app, a.shots, "03-locked")

        # --- open with (second instance) ---------------------------------------------------------
        rc = app.second_instance(vault)
        check("second instance exits and forwards", rc == 0, rc)
        app.wait('!document.getElementById("view-open").hidden && !document.getElementById("pickedVault").hidden', what="vault forwarded")
        check("forwarded vault is selected", app.text("pickedName") == vault.name)
        app.type("openPw", "wrong password")
        app.click("btnUnlock")
        app.wait('!document.getElementById("openMsg").hidden', what="wrong pw")
        check("wrong password message", "Wrong password" in app.text("openMsg"), app.text("openMsg"))
        app.type("openPw", pw)
        app.click("btnUnlock")
        app.wait('!document.getElementById("vaultPane").hidden', what="vault open")
        rows = app.js('return [...document.querySelectorAll("#vaultRows tr")].map(r => r.querySelector(".label").textContent);')
        check("vault lists 6 entries", len(rows) == 6, rows)
        check("entry names shown as text (no HTML)", app.js('return document.querySelectorAll("#vaultRows i").length === 0;'))
        check("password cleared after unlock", app.js('return document.getElementById("openPw").value === "";'))
        check("session pill visible", app.visible("sessionPill"))
        shot(app, a.shots, "04-vault")

        app.type("vaultFilter", "dng")
        n = app.js('return document.querySelectorAll("#vaultRows tr").length;')
        check("filter narrows the list", n == 1, n)
        app.js('document.querySelector("#vaultRows .btn").click(); return true;')
        app.wait('!document.getElementById("vaultResult").hidden', what="single extract")
        out_dng = work / "out" / "IMG_0412.dng"
        check("single file extracted byte-identical", out_dng.exists() and sha(out_dng) == sha(data / "Vacation 2026/raw/IMG_0412.dng"))
        app.type("vaultFilter", "")

        # --- edit the open vault: add, remove, preview, verify, change password -------------------
        note = data / "added-note.txt"
        note.write_text("hello from the vault")
        call = lambda js: app.js(f'return window.__TAURI__.core.invoke({js}).then(r => r, e => ({{error: e.code}}));')
        r = call(f'"edit_vault", {{password: "wrong password", remove: [], add: [{json.dumps(str(note))}], newPassword: null}}')
        check("edit with a wrong password is refused", r.get("error") == "wrong_password", r)
        r = call(f'"edit_vault", {{password: {json.dumps(pw)}, remove: ["passport.pdf"], add: [{json.dumps(str(note))}], newPassword: null}}')
        paths = [e["path"] for e in r.get("entries", [])]
        check("file added and file removed", "added-note.txt" in paths and "passport.pdf" not in paths, paths)
        idx = next((e["index"] for e in r.get("entries", []) if e["path"] == "added-note.txt"), -1)
        text = app.js(f'return window.__TAURI__.core.invoke("preview_entry", {{index: {idx}}}).then(b => new TextDecoder().decode(b));')
        check("preview reads the file from memory", text == "hello from the vault", text)
        r = call('"verify_vault"')
        check("verify checks every byte", isinstance(r, int) and r > 0, r)
        new_pw = pw + " changed"
        r = call(f'"edit_vault", {{password: {json.dumps(pw)}, remove: [], add: [], newPassword: {json.dumps(new_pw)}}}')
        check("password changed and vault reopened", "entries" in r, r)
        app.js('return window.__TAURI__.core.invoke("lock_vault").then(() => 1);')
        r = call(f'"open_vault", {{path: {json.dumps(str(vault))}, password: {json.dumps(pw)}}}')
        check("old password no longer opens it", r.get("error") == "wrong_password", r)
        r = call(f'"open_vault", {{path: {json.dumps(str(vault))}, password: {json.dumps(new_pw)}}}')
        check("new password opens it", "entries" in r, r)
        pw = new_pw

        app.run("btnExtractAll", "extract all")
        root = work / "out" / vault.stem
        check("extract all creates a folder named after the vault", root.is_dir())
        check("extracted tree matches", sha(root / "Vacation 2026/IMG_0412.jpg") == sha(data / "Vacation 2026/IMG_0412.jpg")
              and (root / "<i>notes.txt").read_text() == "html-looking name")
        app.run("btnExtractAll", "extract again")
        check("second extraction does not overwrite", (work / "out" / f"{vault.stem} (2)").is_dir())
        app.run("btnExportZip", "zip")
        check("ZIP export written", (work / "out" / f"{vault.stem}.zip").exists(), app.text("vaultMsg"))

        app.click("btnLock")
        app.wait('document.getElementById("vaultPane").hidden', what="locked")
        r = app.js('return window.__TAURI__.core.invoke("extract_all").then(() => "ok", e => e.code);')
        check("after Lock the key is gone in the backend", r == "locked", r)

        # --- legacy + malicious vaults -------------------------------------------------------------
        app.second_instance(FIX / "v1_evil.pvault")
        app.wait('document.getElementById("pickedName").textContent === "v1_evil.pvault"', what="evil forwarded")
        app.type("openPw", "evil")
        app.click("btnUnlock")
        app.wait('!document.getElementById("vaultPane").hidden', what="evil open")
        check("legacy notice for 1.x vault", app.visible("legacyNote"))
        check("unsafe entries warned", app.visible("unsafeNote") and "2 entries" in app.text("unsafeNote"), app.text("unsafeNote"))
        check("unsafe rows have no Save button", app.js('return [...document.querySelectorAll("#vaultRows tr.unsafe")].every(r => ![...r.querySelectorAll("button")].some(b => b.textContent === t("vault.save")));'))
        check("XSS payload name is inert text", app.js('return document.querySelectorAll("#vaultRows img").length === 0 && document.body.textContent.includes("<img src=x onerror=alert(1)>.txt");'))
        shot(app, a.shots, "05-evil-vault")
        app.run("btnExtractAll", "evil extract")
        check("evil vault extraction reports success", app.text("vaultResultText").startswith("Extracted"), app.text("vaultMsg"))
        check("traversal entries not written outside", not (work / "evil.txt").exists() and not (work / "out" / "evil.txt").exists())
        app.click("btnLock")

        # --- cancel a long operation ----------------------------------------------------------------
        app.click("btnClearSel") if app.visible("btnClearSel") else None
        app.click("encAgain") if app.visible("encAgain") else None
        app.js('document.querySelector(\'[data-view="encrypt"]\').click(); return true;')
        app.js(f'return window.__TAURI__.core.invoke("inspect_paths", {{paths: [{json.dumps(str(big))}]}}).then(i => {{ state.selection = i; renderSelection(); return i.length; }});')
        app.type("encPw", pw)
        app.type("encPw2", pw)
        app.click("btnEncrypt")
        app.wait('!document.getElementById("busy").hidden && document.getElementById("busyPhase").textContent.startsWith("Encrypting")', what="encrypting")
        app.wait('parseInt(document.getElementById("busyPct").textContent) >= 3', what="some progress")
        shot(app, a.shots, "06-progress")
        app.click("btnCancel")
        app.wait('document.getElementById("busy").hidden', what="cancelled")
        check("cancel reports cancelled", "Cancelled" in app.text("encMsg"), app.text("encMsg"))
        check("cancel leaves no vault and no partial file",
              not (work / "out" / "big.pvault").exists() and not list((work / "out").glob(".*pvpart")))

        # --- settings: theme + language -------------------------------------------------------------
        app.js('document.querySelector(\'[data-view="settings"]\').click(); return true;')
        app.js('document.querySelector(\'input[name=theme][value=dark]\').click(); return true;')
        app.wait('document.documentElement.dataset.theme === "dark"', what="dark theme")
        app.js('const s = document.getElementById("setLang"); s.value = "tr"; s.dispatchEvent(new Event("change", {bubbles: true})); return true;')
        app.wait('document.getElementById("setTitle").textContent === "Ayarlar"', what="turkish")
        app.js('document.querySelector(\'[data-view="encrypt"]\').click(); return true;')
        check("result text re-translated on language change", "dosya" in app.text("encResultText"), app.text("encResultText"))
        app.js('document.querySelector(\'[data-view="settings"]\').click(); return true;')
        saved = json.loads((work / "config/privavault/settings.json").read_text())
        check("dark theme + Turkish persisted", saved["theme"] == "dark" and saved["language"] == "tr", saved)
        shot(app, a.shots, "07-settings-dark")
        app.js('document.querySelector(\'[data-view="encrypt"]\').click(); return true;')
        shot(app, a.shots, "08-lock-files-dark")
    finally:
        app.close()
        if not a.keep:
            shutil.rmtree(work, ignore_errors=True)

    failed = [r for r in results if not r[1]]
    print(f"\n{len(results) - len(failed)}/{len(results)} checks passed")
    sys.exit(1 if failed else 0)


if __name__ == "__main__":
    main()

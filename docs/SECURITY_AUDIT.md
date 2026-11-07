# PrivaVault — Security Audit & Threat Model

Scope: PrivaVault 1.4.0 (Electron, commit `a5a470a`) and the 2.0 rework on branch
`tauri-cross-platform`. This document records what was found, what was changed, the evidence
for each fix, and what remains. **It is not a third-party audit.** No claim in here should be
read as "PrivaVault is secure"; it describes specific properties with tests that support them.

## 1. Threat model

### Assets
* Contents and names of files inside a vault.
* The vault password and the keys derived from it.

### What PrivaVault protects against
| Adversary | Example | Protection |
| --- | --- | --- |
| **Offline attacker with the vault file** | Stolen laptop, leaked backup, cloud copy | AES-256-GCM; scrypt (≥128 MiB) makes password guessing expensive |
| **Someone who modifies the vault** | Bit-flips, truncation, chunk reordering, header edits | Every 64 KiB chunk is authenticated *before* its plaintext is released; header is bound to every chunk; truncation/extension detected |
| **A malicious vault you are asked to open** (with its password) | "Here is the file, password is 1234" | Entry names are validated (no `../`, absolute paths, drive prefixes, symlinks); names are rendered as text only (no HTML); KDF parameters are bounded so a header cannot freeze the app or exhaust RAM |
| **Network observers** | ISP, Wi-Fi | No network traffic unless the user enables the update check |

### Out of scope (cannot be solved by an app like this)
* Malware or another program running as the same user (keyloggers, memory readers, screen capture).
* An attacker with physical access to the **unlocked, running** machine.
* Weak passwords: a short or reused password can be guessed regardless of the KDF.
* Plaintext *you* extract: once saved, it is an ordinary file. Originals are never deleted by
  PrivaVault, and the app cannot securely erase them (SSDs and copy-on-write filesystems such as
  Btrfs — the CachyOS default — make "secure delete" unreliable). Use full-disk encryption.
* Metadata visible without the password: vault file size and modification time.

## 2. Findings in 1.4.0

Severity is relative to the threat model above.

| # | Finding | Severity | Status in 2.0 |
| --- | --- | --- | --- |
| F1 | **Stored XSS**: entry names from the vault were inserted with `innerHTML` and inside an inline `onclick='…'`. A crafted vault ran script in the renderer, which could read the password field and call every `electronAPI` method. | High | **Fixed** — UI builds DOM with `textContent` only; test asserts no `innerHTML`/`eval`/`insertAdjacentHTML` in the UI; CSP forbids inline script. |
| F2 | **Remote script** `canvas-confetti` from jsDelivr, no SRI, no CSP. A CDN compromise = code execution with access to passwords. | High | **Fixed** — removed; all assets bundled; CSP `script-src 'self'`; test asserts no remote resources. |
| F3 | **Undisclosed network call** to `api.github.com` on every start (README claimed "no network calls"). | Medium (privacy) | **Fixed** — opt-in setting, off by default; UI shows network status permanently in the sidebar. |
| F4 | **Plaintext temp files**: every inspect/extract wrote the entire decrypted ZIP to `%TEMP%`/`/tmp`, deleted with `unlink` only; survives crashes. | High | **Fixed** — no temp files at all (v2: per-chunk; v1: seekable CTR reader feeding the ZIP parser). |
| F5 | **Unauthenticated plaintext released**: GCM tag checked only after the whole plaintext had been written. | Medium | **Fixed** — v2 authenticates each chunk before writing; v1 authenticates the full file (GHASH over ciphertext) before decrypting any byte. Tests: `v1_tampering_is_detected_before_any_output`, `v2_tampering_is_detected`. |
| F6 | **DoS via header**: iteration count read from the file without bounds; `0xFFFFFFFF` froze the main process for hours (`pbkdf2Sync` on the UI thread). | Medium | **Fixed** — v1 iterations ≤ 10 M, scrypt params bounded (≤ 1 GiB); KDF runs on a worker thread. Test: `v1_tampered_iteration_count_cannot_freeze_the_app`, `v2_header_params_are_bounded`. |
| F7 | No `sandbox`, no CSP, no navigation restrictions on the BrowserWindow. | Medium | **Fixed by design** — Tauri: strict CSP, `freezePrototype`, capability file exposes only event/window basics; dialogs, file I/O, notifications and URL opening are Rust-side only. |
| F8 | Electron 28 end-of-life (unpatched Chromium). | High | **Fixed** — Electron removed; system WebView (WebKitGTK / WebView2) is updated by the OS. |
| F9 | Weak default: PBKDF2 100k iterations, setting not persisted. | Medium | **Fixed** — v2 uses scrypt N=2¹⁷ r=8 (128 MiB) by default, up to 512 MiB; persisted setting. |
| F10 | Zip-slip protection delegated entirely to `node-stream-zip`. | Medium | **Fixed** — own validator (`paths::sanitize_entry`) for both formats; unsafe entries are shown, marked, and never written. Test: `v1_malicious_entry_names_are_never_extracted` (real crafted vault). |
| F11 | All errors reported as "Incorrect password" (disk full looked like a wrong password; users may retry/give up wrongly). | Low | **Fixed** — stable error codes; wrong password vs damage vs I/O are distinguished. |
| F12 | Partial outputs left after failures; extraction could overwrite existing files. | Medium (integrity) | **Fixed** — atomic `*.pvpart` + rename; extraction goes into a new folder and never clobbers. |
| F13 | Symlinks inside selected folders followed (loops, unintended content). | Low | **Fixed** — not followed; reported as skipped. |
| F14 | Header (salt/IV/iterations) not authenticated. | Low | v1: tampering changes the key → tag fails (acceptable). v2: header is the AAD of every chunk and covered by an HMAC key check. |
| F15 | Window "close" hid the app to the tray; on desktops without a tray it kept running invisibly with the key material of the last operation's process. | Low | **Fixed** — closing quits (asks first if an operation is running); vault is locked on close. |

## 3. Cryptographic design (2.0)

No custom algorithms are used. Primitives come from the RustCrypto project
(`aes-gcm` 0.10, `aes` 0.8, `ctr` 0.9, `ghash` 0.5, `aead` 0.5 `stream`, `scrypt` 0.11,
`pbkdf2` 0.12, `hkdf` 0.12, `hmac` 0.12, `sha2` 0.10) and randomness from the OS (`getrandom`).

```
master   = scrypt(password, salt₁₆, N, r, p) → 32 bytes          (params in header, bounded)
K_payload = HKDF-SHA256(master, info="PrivaVault v2 payload")
K_check   = HKDF-SHA256(master, info="PrivaVault v2 key check")
key_check = HMAC-SHA256(K_check, header[0..64])[0..16]             (stored at header[64..80])
chunk i   = AES-256-GCM(K_payload, nonce = prefix₇ ‖ BE32(i) ‖ last?, aad = header[0..80])
```

* **STREAM** (Hoang–Reyhanitabar–Rogaway–Vizár 2015), via `aead::stream::StreamBE32`: chunk order and
  truncation are authenticated. The nonce prefix is random per vault and the key is unique per vault
  (random salt), so nonce reuse across vaults is not a concern; within a vault the counter never repeats.
* **Key check**: lets the app say "wrong password" accurately without decrypting data. It reveals nothing
  an attacker could not learn by trying to decrypt the first chunk, so it does not make guessing cheaper.
* **Wrong password vs. tampered header**: a modified header also fails the key check and is reported as
  "wrong password". This is a deliberate simplification; it never results in data being released.
* **v1 compatibility**: GCM is reproduced as CTR (from inc32(J0)) + GHASH, the same composition `aes-gcm`
  uses internally. The test `streaming_gcm_matches_aes_gcm_crate` compares tags and plaintext with
  `aes-gcm` for lengths 0…3 MiB+7; the fixtures in `tests/fixtures` were produced by the original 1.4.0
  JavaScript code.
* KDF test vectors: RFC 7914 §11 (PBKDF2-HMAC-SHA256) and §12 (scrypt).

## 4. Secrets in memory

* Derived keys and HKDF outputs are `Zeroizing<[u8; 32]>` and wiped on drop; plaintext chunk buffers and
  the decrypted manifest are `Zeroizing<Vec<u8>>`. The Rust side wraps received passwords in `Zeroizing`.
* The `zeroize` features of `aes`, `aes-gcm` and `ctr` are enabled, so expanded AES key schedules are
  wiped when the cipher objects are dropped (e.g. on *Lock*).
* **Limitations (honest):** JavaScript
  strings (the password field value, the IPC message) cannot be wiped and live until garbage-collected;
  the OS may swap memory to disk (use encrypted swap / zram — CachyOS uses zram by default).
* While a vault is **open**, its key stays in memory so files can be extracted without re-entering the
  password. It is dropped on *Lock*, after the idle auto-lock (default 10 min), on opening another vault,
  and on quit.

## 5. Files, permissions, logging, network

* Vaults, extracted files and settings are created with mode `0600` (`0700` for executables and for the
  extraction root and config dir) on Linux. On Windows they inherit the destination's ACL (per-user under
  the profile by default).
* Settings (`~/.config/privavault/settings.json` / `%APPDATA%\PrivaVault\settings.json`) hold only UI
  preferences — no paths, no history, no secrets. A corrupt file is moved aside, never trusted.
* Logging: no log files are written. Error details printed to the developer console contain error kinds and
  file paths only — never passwords, keys or content (the core's `Error` type has no variant that can hold them).
* Network: none by default. With "Check for updates" on, one `GET https://api.github.com/repos/TNFX1/PrivaVault/releases/latest`
  per launch, without cookies or referrer. The CSP allows no other origin.
* **Renderer cannot choose output paths.** Every destination comes from a native dialog opened by Rust.
  A compromised renderer could still *read* files by asking to encrypt them into a vault, but the user would
  see the save dialog.

## 6. Dependencies

* JavaScript runtime dependencies: **none** (was: archiver, node-stream-zip, Electron, a CDN script).
* Rust: `cargo tree` for the full list; RustCrypto crates are widely used and partly audited
  (NCC Group audited `aes-gcm`/`aes` in 2020).
* Recommended in CI: `cargo audit` (RustSec advisories). Not run in this session (tool not installed);
  see Remaining work.

## 7. Remaining risks & follow-ups

1. **No independent cryptographic review** of the v2 container format. Recommended before advertising it widely.
2. Memory hygiene is best-effort: Rust may leave copies during reallocation, and JS strings can't be wiped.
3. Header tampering is reported as "wrong password" (see §3).
4. v1 vaults: full-file authentication followed by decryption is two reads of the file; a local attacker able to
   modify the file *between* the two passes is out of scope (same-user attacker).
5. Password strength meter is a heuristic, not zxcvbn.
6. Windows: ACLs are inherited rather than explicitly restricted.
7. Windows build, installer and file association were configured but **not executed on Windows in this session**;
   CI (`windows-latest`) is set up to run the tests there.
8. No GUI end-to-end automation yet (WebKitWebDriver not available on the dev machine); GUI flows were
   checked manually — see `docs/BUILDING.md#manual-gui-checklist`.

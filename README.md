<div align="center">
  <img src="assets/logo.png" alt="PrivaVault logo" width="120">
  <h1>PrivaVault</h1>
  <p><b>Offline file encryption for Linux and Windows.</b><br>
  Lock files and folders into a password-protected vault on your own device. No account, no cloud, no telemetry.</p>
  <p>
    <a href="https://github.com/TNFX1/PrivaVault/releases"><img src="https://img.shields.io/github/v/release/TNFX1/PrivaVault?color=2f5bea&label=release" alt="Release"></a>
    <img src="https://img.shields.io/badge/platform-Linux%20%7C%20Windows-2f5bea" alt="Platforms">
    <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-2f5bea" alt="MIT"></a>
  </p>
</div>

<p align="center"><img src="assets/preview.png" alt="PrivaVault window" width="760"></p>

## What it does

* **Lock files:** pick files and folders (or drag them in), choose a password, and PrivaVault writes one
  encrypted `.pvault` file. Your originals are left untouched.
* **Open a vault:** see what's inside instantly, save single files, extract everything into a new folder,
  or save a plain ZIP.
* **Stays offline:** nothing leaves your computer. The only optional network request is an update check,
  which is off until you turn it on.
* **Opens 1.x vaults:** every vault made by PrivaVault 1.x still opens.

## How your files are protected

* Your password is turned into a key with **scrypt** (128 MiB of memory by default, up to 512 MiB), which
  makes guessing passwords slow and expensive.
* Contents are encrypted with **AES-256-GCM** in 64 KiB chunks. Every chunk is checked for tampering
  *before* any of it is written to disk; reordering, cutting or editing a vault is detected.
* Nothing decrypted is ever written to a temp folder. Unsafe file names inside a vault (such as `../`) are
  refused.
* There is **no password recovery**. If you lose the password, the vault cannot be opened by anyone.

Details, threat model and known limitations: [docs/SECURITY_AUDIT.md](docs/SECURITY_AUDIT.md). PrivaVault
has not had an independent security audit.

## Install

| Platform | Download / command | Guide |
| --- | --- | --- |
| Arch Linux, CachyOS | `cd packaging/arch && PRIVAVAULT_LOCAL_SRC=../.. makepkg -si` | [Linux setup](docs/LINUX_SETUP.md) |
| Debian, Ubuntu | `.deb` from [Releases](https://github.com/TNFX1/PrivaVault/releases) | [Linux setup](docs/LINUX_SETUP.md) |
| Fedora, openSUSE | `.rpm` from Releases | [Linux setup](docs/LINUX_SETUP.md) |
| Any Linux | `.AppImage` from Releases | [Linux setup](docs/LINUX_SETUP.md) |
| Windows 10/11 | `PrivaVault_x64-setup.exe` or `PrivaVault-Portable.exe` | [Windows setup](docs/WINDOWS_SETUP.md) |

## Build from source

```bash
git clone https://github.com/TNFX1/PrivaVault.git && cd PrivaVault
cargo test --workspace          # run the test suite
cargo run -p privavault         # start the app
```

Linux needs WebKitGTK 4.1 development files; see [docs/BUILDING.md](docs/BUILDING.md) for all platforms
and release packaging.

## Project layout

```
crates/privavault-core   vault format, encryption, safe extraction (Rust library)
src-tauri                the desktop app (Tauri 2)
ui                       interface (HTML/CSS/JS, bundled, no network)
docs                     architecture, security audit, setup and build guides
```

More: [Architecture](docs/ARCHITECTURE.md) · [Project audit](docs/PROJECT_AUDIT.md) · [Changelog](docs/CHANGELOG.md)

## FAQ

**I forgot my password.** It cannot be recovered or reset. Keep it in a password manager.

**Can PrivaVault 1.x open vaults made by 2.0?** No. 2.0 uses a stronger format. 2.0 opens both.

**Does it delete my original files?** No. Delete them yourself once the vault is stored safely. On SSDs and
copy-on-write filesystems, deleting a file doesn't erase it from the disk; full-disk encryption covers that.

**Is there a size limit?** No practical one: files are processed in small chunks with constant memory use.

## License

MIT, see [LICENSE](LICENSE). The bundled IBM Plex Sans font is under the SIL Open Font License
([ui/fonts/LICENSE-OFL.txt](ui/fonts/LICENSE-OFL.txt)).

*Provided as is, without warranty. Keep backups of important files and your passwords.*

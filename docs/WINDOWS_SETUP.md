# PrivaVault on Windows 10 / 11

> **Status:** the Windows build is configured (NSIS installer + portable exe) and Windows tests run in
> CI on `windows-latest`. As of 2.0.0 the Windows build has **not** yet been run by hand on a Windows
> machine; please report anything that looks wrong.

## Install

* **Installer:** `PrivaVault_2.0.0_x64-setup.exe` — per-user install (no admin rights), Start-menu
  entry, uninstaller, registers the `.pvault` file type. English and Turkish installer languages.
* **Portable:** `PrivaVault-Portable-2.0.0.exe` — run directly. Settings are still stored in
  `%APPDATA%\PrivaVault` (not next to the exe).

### WebView2
PrivaVault uses Microsoft Edge WebView2, which is part of Windows 11 and of up-to-date Windows 10.
If it is missing, the installer downloads the official bootstrapper. The portable exe needs it to be
installed already ([download](https://developer.microsoft.com/microsoft-edge/webview2/)).

## Where things are

| What | Path |
| --- | --- |
| Settings | `%APPDATA%\PrivaVault\settings.json` |
| Program (installer) | `%LOCALAPPDATA%\PrivaVault\` |
| Temp files | none; in-progress outputs are `.<name>.<random>.pvpart` next to the destination |

## Upgrading from PrivaVault 1.x (Electron)

* Uninstall 1.x from *Settings → Apps* (the two versions have different installers), then install 2.0.
* Every `.pvault` created by 1.x opens in 2.0 (read-only format support). New vaults use the 2.0 format,
  which 1.x cannot open.
* 1.x stored no settings on disk, so nothing needs migrating.

## Notes

* File permissions: files inherit the ACL of the folder you save into (your profile folders are private
  to your account by default).
* Entry names that Windows cannot represent (`CON`, `a:b`, trailing dots, `<>|?*"`) are listed but
  skipped on extraction; extract them on Linux if needed.
* SmartScreen may warn about an unsigned installer. Code signing is not set up yet.

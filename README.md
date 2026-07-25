<div align="center">
  <img src="logo.png" alt="PrivaVault Logo" width="160" height="160" style="border-radius: 20px;">
  
  # PrivaVault 🔒
  
  <p align="center">
    <b>Modern, Secure, and Zero-Knowledge Browser-Based File Encryption Tool</b><br>
    <i>Local & Zero-Knowledge File Encryption Tool</i>
  </p>

  <p align="center">
    <img src="https://img.shields.io/badge/Version-1.2-blue?style=flat-square" alt="Version">
    <img src="https://img.shields.io/badge/Security-AES--256--GCM-success?style=flat-square" alt="Encryption">
    <img src="https://img.shields.io/badge/Architecture-Single__File-informational?style=flat-square" alt="Single File">
    <img src="https://img.shields.io/badge/Privacy-100%25_Local-orange?style=flat-square" alt="Privacy">
    <img src="https://img.shields.io/badge/License-MIT-lightgrey?style=flat-square" alt="License">
  </p>
</div>

---

## 🌟 Overview

**PrivaVault** is a modern, secure, and lightning-fast security tool designed to encrypt and decrypt your sensitive files completely locally within your browser, requiring zero external servers or complex backends. Built on a strict **Single-File Architecture**, the entire application logic, styles, and assets are bundled into a single standalone `index.html` file. 

By design, your data never leaves your local device, is never transmitted over the network, and is never stored or processed on third-party servers.

---

## ✨ Why Single-File Architecture?

PrivaVault intentionally avoids traditional heavy frameworks, external CDNs, and complex build steps to provide unique security and reliability benefits:
- 🛡️ **Zero Supply Chain Risks:** No external NPM packages or external scripts means zero risk of third-party compromises.
- 🌐 **100% Offline Capability:** Once saved, it runs seamlessly anywhere without requiring an internet connection.
- ⚡ **Instant Execution:** No server deployment, hosting configurations, or Node.js installations required—just open and run.

---

## 🛠️ Key Features

| Feature | Description |
| :--- | :--- |
| 📁 **Single-File Design** | Everything you need is self-contained within a single `index.html` file. |
| 🔐 **Zero-Knowledge Architecture** | Cryptographic operations run locally via the native Web Crypto API. Your passwords and data are never exposed. |
| 🛡️ **Strong Encryption** | Powered by industry-standard **AES-256-GCM** encryption and **PBKDF2-SHA256** key derivation algorithms. |
| 🌐 **Multi-Language Support** | Built-in support for **English** (`EN`) and **Turkish** (`TR`) for a seamless user experience. |
| ⚙️ **Customizable Security** | Adjustable PBKDF2 iteration counts (100k, 300k, 600k) and custom file extension choices (`.enc`, `.pvault`, etc.). |

---

## 📦 Installation & Usage

Because PrivaVault is a standalone **single-file** application, you don't need any complex setup or Node.js environment. You can get started in seconds:

### Option 1: Download Standalone File (Recommended for Users)
1. Go to the [Releases](https://github.com/TNFX1/PrivaVault/releases) page.
2. Download the latest **`index.html`** file directly.
3. Double-click the file to open it in any modern web browser (Chrome, Firefox, Edge, Safari).

### Option 2: Clone Repository (For Developers)
1. Clone the repository to your local machine:
   ```bash
   git clone [https://github.com/TNFX1/PrivaVault.git](https://github.com/TNFX1/PrivaVault.git)
   ```
2. Open the **`index.html`** file inside the cloned folder with your browser.

---

## 📖 Usage Guide

### 🔐 File Encryption
1. Select the **Encrypt File** tab from the interface.
2. Click **Browse...** to choose the file you wish to secure.
3. Enter a secure password and confirm it in the confirmation field.
4. Click **Encrypt & Download** to instantly download your securely locked file.

### 🔓 File Decryption
1. Select the **Decrypt File** tab from the interface.
2. Upload your encrypted file (`.enc`, `.pvault`, etc.) to the system.
3. Enter the password used during the encryption process.
4. Click **Decrypt & Download** to restore your original file with its accurate name.

---

## 🛡️ Security Architecture

- **AES-GCM (Galois/Counter Mode):** Ensures both confidentiality and data integrity, immediately detecting any unauthorized tampering with encrypted data.
- **Randomized Salt & IV:** Cryptographically secure random **Salt** (16-byte) and **IV** (12-byte) values are generated on every encryption operation, ensuring unique encrypted outputs even when using the same password.

---

## 📄 License

This project is licensed under the [MIT License](LICENSE). Feel free to customize, modify, and contribute.

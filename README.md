<div align="center">
  <img src="logo.png" alt="PrivaVault Logo" width="160" height="160" style="border-radius: 28px; box-shadow: 0 10px 30px rgba(0,0,0,0.5);">
  
  # PrivaVault
  
  <p><strong>Secure, Local, and Zero-Knowledge Client-Side File Encryption</strong></p>

  <p>
    <a href="https://opensource.org/licenses/MIT"><img src="https://img.shields.io/badge/License-MIT-yellow.svg?style=flat-square" alt="License: MIT"></a>
    <a href="https://caniuse.com/cryptography"><img src="https://img.shields.io/badge/Web%20Crypto-API-blue?style=flat-square" alt="Web Crypto API"></a>
    <img src="https://img.shields.io/badge/Status-Active-success?style=flat-square" alt="Status">
    <img src="https://img.shields.io/badge/Privacy-100%25_Client_Side-critical?style=flat-square" alt="Privacy">
  </p>

  <p>
    <a href="#-features">Features</a> •
    <a href="#️-security-architecture">Security</a> •
    <a href="#-how-to-use">How to Use</a> •
    <a href="#-tech-stack">Tech Stack</a> •
    <a href="#-license">License</a>
  </p>
</div>

---

## 🌟 Overview

**PrivaVault** is a high-security, lightweight web application built to encrypt and decrypt files directly inside your browser. Designed with a strict **Zero-Knowledge** architecture, it ensures that your data, keys, and passwords never leave your device and are never transmitted to any external server.

Whether you want to securely store sensitive documents on public clouds or share confidential files across untrusted channels, PrivaVault gives you total cryptographic data sovereignty.

---

## ✨ Features

* **100% Client-Side Processing:** All cryptographic operations run locally in your browser memory using the native **Web Crypto API**. No backend, no databases, no tracking.
* **Military-Grade Encryption:** Powered by **AES-256-GCM** (Galois/Counter Mode), providing both high-performance confidentiality and authenticated integrity.
* **Advanced Key Stretching:** Uses **PBKDF2-SHA256** with customizable security iterations (up to 600,000 rounds) to defeat brute-force and GPU cracking attempts.
* **Multi-Language Support:** Full built-in support for **English (EN)** and **Turkish (TR)** with persistent user preferences (`localStorage`).
* **Customizable Extensions:** Choose your preferred encrypted file extension (`.enc`, `.pvault`, `.locked`, `.secure`, `.vault`, `.crypto`).
* **Air-Gap / Offline Capable:** Works entirely offline without an active internet connection.

---

## 🛡️ Security Architecture

PrivaVault follows a strict **"Trust No One"** model:
1. **Key Derivation:** Your password is never stored or transmitted. It is combined with a cryptographically secure random 128-bit `salt` through PBKDF2 to derive a strong 256-bit AES key.
2. **Initialization Vector (IV):** Every encryption session generates a unique, unpredictable 96-bit random IV, ensuring identical files encrypted with the same password yield completely different ciphertexts.
3. **Data Integrity:** AES-GCM guarantees that if an attacker modifies even a single bit of the encrypted file, decryption will mathematically fail, preventing tampering.

---

## 🚀 How to Use

### Option 1: Online (via GitHub Pages)
Visit the live hosted version directly:
👉 **[PrivaVault Live App](https://tnfx1.github.io/PrivaVault/)**

### Option 2: Locally / Offline (Maximum Privacy & Air-Gapped)
1. Clone or download this repository.
2. Ensure `index.html` and `logo.png` are in the same folder.
3. Double-click `index.html` to open it in any modern browser (Chrome, Edge, Firefox, Safari).
4. *Tip:* You can disconnect your internet entirely before using it for maximum security.

---

## 🛠️ Tech Stack

* **HTML5 / CSS3:** Clean, responsive, modern dark-mode UI.
* **JavaScript (ES6+):** Pure vanilla JS with a modular structure.
* **Web Crypto API:** Native browser cryptographic engine.

---

## 📄 License

This project is open-source and available under the terms of the [MIT License](LICENSE).

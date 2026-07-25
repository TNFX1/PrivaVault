# 🔒 PrivaVault

**PrivaVault** is a secure, lightweight, and zero-knowledge local file encryption web application. It allows you to encrypt and decrypt your files directly inside your browser using industry-standard cryptographic algorithms.

🌐 **Live Demo:** [PrivaVault Live App](https://tnfx1.github.io/PrivaVault/)

---

## ✨ Features

* **100% Client-Side Processing:** All encryption and decryption processes run locally in your browser memory using the native **Web Crypto API**. Your data, keys, and passwords **never** leave your device or get sent to any server.
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
Visit the live hosted link: [PrivaVault Web App](https://tnfx1.github.io/PrivaVault/)

### Option 2: Locally / Offline (Maximum Privacy)
1. Download or clone this repository.
2. Ensure `index.html` and `logo.png` are in the same folder.
3. Double-click `index.html` to open it in any modern web browser (Chrome, Edge, Firefox, Safari).
4. *Tip:* You can disconnect your internet entirely before using it for maximum security.

---

## 🛠️ Tech Stack

* **HTML5 / CSS3:** Clean, responsive, modern dark-mode UI.
* **JavaScript (ES6+):** Pure vanilla JS with a modular structure.
* **Web Crypto API:** Native browser cryptographic engine.

---

## 📄 License

This project is open-source and available under the terms of the [MIT License](LICENSE).

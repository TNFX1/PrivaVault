<div align="center">

&#x20; <img src="logo.png" alt="PrivaVault Logo" width="90" height="90" style="border-radius: 20px;">

&#x20; <h1>PrivaVault</h1>

&#x20; <p><strong>Secure, Local, and Zero-Knowledge Client-Side File Encryption</strong></p>



&#x20; <p>

&#x20;   <a href="https://opensource.org/licenses/MIT"><img src="https://img.shields.io/badge/License-MIT-yellow.svg" alt="License: MIT"></a>

&#x20;   <a href="https://caniuse.com/cryptography"><img src="https://img.shields.io/badge/Web%20Crypto-API-blue" alt="Web Crypto API"></a>

&#x20;   <img src="https://img.shields.io/badge/Status-Active-success" alt="Status">

&#x20; </p>



&#x20; <p>

&#x20;   <a href="#-features">Features</a> •

&#x20;   <a href="#-security-architecture">Security</a> •

&#x20;   <a href="#-how-to-use">How to Use</a> •

&#x20;   <a href="#-local-development">Development</a> •

&#x20;   <a href="#license">License</a>

&#x20; </p>

</div>



\---



\## 🌟 Overview



\*\*PrivaVault\*\* is a lightweight, high-security web application designed to encrypt and decrypt files directly inside your browser. Built with a \*\*Zero-Knowledge\*\* architecture, it ensures that your data, keys, and passwords never leave your device and are never transmitted to any external server.



Whether you want to store sensitive documents on public clouds safely or send encrypted files across untrusted channels, PrivaVault gives you total cryptographic data sovereignty.



\---



\## ✨ Features



\* \*\*100% Client-Side Processing:\*\* All cryptographic operations run locally in your browser memory using the native \*\*Web Crypto API\*\*. No backend, no databases, no tracking.

\* \*\*Military-Grade Encryption:\*\* Secured by \*\*AES-256-GCM\*\* (Galois/Counter Mode), providing both high-performance confidentiality and authenticated integrity.

\* \*\*Advanced Key Stretching:\*\* Uses \*\*PBKDF2-SHA256\*\* with customizable security iterations (up to 600,000 rounds) to defeat brute-force and GPU cracking attempts.

\* \*\*Multi-Language Interface:\*\* Seamless support for \*\*English (EN)\*\* and \*\*Turkish (TR)\*\* with persistent user preferences (`localStorage`).

\* \*\*Customizable Vault Extensions:\*\* Choose your preferred encrypted file extension (`.enc`, `.pvault`, `.locked`, `.secure`, etc.).

\* \*\*Air-Gap / Offline Ready:\*\* Works entirely offline without an internet connection.



\---



\## 🛡️ Security Architecture



PrivaVault follows a \*\*"Trust No One"\*\* trust model:

1\. \*\*Derivation:\*\* Your password is never stored. It is combined with a cryptographically secure random 128-bit `salt` through PBKDF2 to derive a strong 256-bit AES key.

2\. \*\*Initialization Vector (IV):\*\* Every encryption session generates a unique, unpredictable 96-bit random IV, ensuring identical files encrypted with the same password yield completely different ciphertexts.

3\. \*\*Authentication:\*\* AES-GCM guarantees that if an attacker modifies even a single bit of the encrypted file, decryption will mathematically fail, preventing tampering.



\---



\## 🚀 How to Use



\### Option 1: Live Web App (GitHub Pages)

Visit the official hosted version directly:

👉 \*\*\[PrivaVault Live Demo](https://your-username.github.io/privavault/)\*\* \*(Replace with your URL)\*



\### Option 2: Run Locally (Maximum Privacy \& Air-Gapped)

1\. Clone or download this repository.

2\. Ensure you have `index.html` and `logo.png` in the same folder.

3\. Double-click `index.html` to open it in any modern browser (Chrome, Edge, Firefox, Safari).

4\. \*Tip:\* You can disconnect your Wi-Fi/Internet entirely before using it.



\---



\## 🛠️ Tech Stack



\* \*\*HTML5 / CSS3:\*\* Clean, responsive, modern dark-mode UI.

\* \*\*JavaScript (ES6+):\*\* Pure vanilla JS with modular code structure.

\* \*\*Web Crypto API:\*\* Native browser cryptographic engine (fast, secure, and audited).



\---



\## 📄 License



This project is open-source and available under the terms of the \[MIT License](LICENSE).


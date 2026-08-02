<div align="center">



&#x20; <img src="logo.png" alt="PrivaVault Logo" width="140" height="140">



&#x20; # PrivaVault 🔒



&#x20; \*\*Modern, Secure, and Zero-Knowledge Desktop File Encryption Application\*\* \*High-Performance Local Stream Encryption Powered by Electron \& Node.js\*



&#x20; <br>



&#x20; \[!\[Version](https://img.shields.io/badge/Version-1.3.5-blue?style=for-the-badge)](https://github.com/TNFX1/PrivaVault/releases)

&#x20; \[!\[Security](https://img.shields.io/badge/Security-AES--256--GCM-success?style=for-the-badge)](#-security-architecture)

&#x20; \[!\[Framework](https://img.shields.io/badge/Framework-Electron-informational?style=for-the-badge)](https://electronjs.org)

&#x20; \[!\[Privacy](https://img.shields.io/badge/Privacy-100%25\_Local-orange?style=for-the-badge)](#-overview)

&#x20; \[!\[License](https://img.shields.io/badge/License-MIT-lightgrey?style=for-the-badge)](LICENSE)



</div>



\---



\## 🌟 Overview



\*\*PrivaVault\*\* is a modern, ultra-secure, desktop-native file encryption tool built on \*\*Electron\*\*. It provides high-performance zero-knowledge file encryption completely offline on your device, eliminating external servers, network traffic, or cloud dependencies.



Powered by Node.js stream processing and AES-256-GCM cryptography, PrivaVault allows you to pack multiple files into a single encrypted container (`.pvault` or custom extensions), inspect container contents securely, and selectively extract files without unnecessary overhead.



\---



\## ✨ Key Features



| Feature | Description |

| :--- | :--- |

| 🖥️ \*\*Native Desktop Performance\*\* | Built with Electron \& Node.js streams to process large files with minimal RAM consumption. |

| 🔐 \*\*Zero-Knowledge Privacy\*\* | 100% local processing. Passwords, encryption keys, and raw files never leave your system. |

| 📦 \*\*Vault Multi-File Container\*\* | Bundle multiple files into a single encrypted vault archive with custom extensions (`.pvault`, `.enc`, etc.). |

| 🔍 \*\*Vault Inspection\*\* | Inspect and preview encrypted container contents safely before extracting. |

| 🎯 \*\*Selective \& Batch Extraction\*\* | Extract specific files individually or dump all contents to a folder/ZIP archive. |

| 🛡️ \*\*Military-Grade Security\*\* | \*\*AES-256-GCM\*\* authenticated stream encryption combined with \*\*PBKDF2-SHA256\*\* key derivation. |

| ⚡ \*\*Real-Time Progress Indicator\*\* | Built-in progress tracking during multi-file stream encryption and extraction operations. |



\---



\## 💻 Installation \& Usage



PrivaVault is available as an official Windows installer as well as a standalone portable binary.



\### Option 1: Download Windows Executables (Recommended)

1\. Go to the \[Releases](https://github.com/TNFX1/PrivaVault/releases) page.

2\. Download the latest version:

&#x20;  - \*\*`PrivaVault-Setup-X.X.X.exe`\*\* (Standard Windows Installer)

&#x20;  - \*\*`PrivaVault-X.X.X.exe`\*\* (Portable Executable – No Installation Required)

3\. Run the executable and start securing your files immediately!



\### Option 2: Build from Source (Developers)



```bash

\# Clone the repository

git clone \[https://github.com/TNFX1/PrivaVault.git](https://github.com/TNFX1/PrivaVault.git)



\# Navigate into the project folder

cd PrivaVault



\# Install dependencies

npm install



\# Start the application in development mode

npm start



\# Build Windows executable binaries

npm run build

```



\---



\## 📖 How It Works



\### 🔐 Encrypting Files (Creating a Vault)

1\. Launch \*\*PrivaVault\*\* and navigate to the \*\*Encrypt\*\* tab.

2\. Click \*\*Select Files\*\* to pick one or more files to protect.

3\. Enter your password and choose your preferred PBKDF2 iterations (e.g., 100,000+).

4\. Set a custom container extension if desired (default: `.pvault`).

5\. Click \*\*Encrypt \& Save\*\* to stream-encrypt your package.



\### 🔓 Inspecting \& Extracting Vaults

1\. Go to the \*\*Decrypt / Vault Manager\*\* tab.

2\. Load your encrypted `.pvault` file and enter your password.

3\. \*\*Inspect Vault:\*\* View the embedded file tree and individual file sizes.

4\. \*\*Extract Single File:\*\* Extract only the selected file directly to your target destination.

5\. \*\*Extract All:\*\* Extract the entire container into a folder or unencrypted `.zip` archive.



\---



\## 🛡️ Security Architecture



PrivaVault adheres to strict cryptographic standards:



\- \*\*AES-256-GCM (Galois/Counter Mode):\*\* Ensures confidentiality and authenticated data integrity. Any byte-level modification or corrupt file structure is detected instantly.

\- \*\*PBKDF2-SHA256 Key Derivation:\*\* Password-based key derivation using high iteration counts to thwart brute-force and dictionary attacks.

\- \*\*Cryptographic Headers:\*\* Each vault file contains a dynamic header storing unique 16-byte Salt, 12-byte IV, 4-byte Iteration metadata, and a 16-byte Authentication Tag appended at the end.



\---



\## 📄 License



This project is open-source and released under the \[MIT License](LICENSE). Feel free to fork, modify, and contribute!


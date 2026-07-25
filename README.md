# PrivaVault 🔒

<p align="center">
  <img src="logo.png" alt="PrivaVault Logo" width="120" height="120">
</p>

<p align="center">
  <b>Secure, Local & Zero-Knowledge File Encryption Tool</b><br>
  <i>Sıfır Bilgi Mimarisi ile Tamamen Tarayıcınızda Çalışan Yerel Dosya Şifreleme Aracı</i>
</p>

<p align="center">
  <a href="#features--özellikler">Features</a> •
  <a href="#how-it-works--nasıl-çalışır">How It Works</a> •
  <a href="#security--güvenlik">Security</a> •
  <a href="#usage--kullanım">Usage</a> •
  <a href="#license--lisans">License</a>
</p>

---

## 🚀 About / Hakkında

**PrivaVault** is a lightweight, secure, and modern web application that allows you to encrypt and decrypt your sensitive files locally right inside your browser. No server uploads, no third-party tracking, and absolute zero-knowledge privacy.

**PrivaVault**, hassas dosyalarınızı harici bir sunucuya göndermeden, tamamen tarayıcınız içinde yerel olarak şifrelemenize ve şifre çözmenize olanak tanıyan hafif, güvenli ve modern bir web uygulamasıdır. Verileriniz asla cihazınızdan dışarı çıkmaz.

---

## ✨ Features / Özellikler

- **100% Client-Side (Zero-Knowledge):** All cryptographic operations run locally using the native Web Crypto API. Your files and passwords never leave your device. *(Tüm işlemler tarayıcınızda gerçekleşir, verileriniz asla sunucuya gitmez.)*
- **Strong Encryption Standards:** Powered by **AES-256-GCM** for encryption and **PBKDF2-SHA256** for secure key derivation.
- **Multi-Language Support:** Built-in support for **English** and **Turkish** (`EN` / `TR`).
- **Customizable Security Settings:** - Adjustable PBKDF2 iteration counts (100k, 300k, 600k iterations).
  - Custom encrypted file extensions (`.enc`, `.pvault`, `.locked`, `.secure`, `.vault`, `.crypto`).
- **Modern & Responsive UI:** Clean, dark-themed interface with smooth tab navigation, auto-clear form security feature, and embedded base64 branding.

---

## 🛠️ Technology Stack / Teknolojiler

- **HTML5 / CSS3** (Custom Modern CSS Variables & Responsive Design)
- **Vanilla JavaScript (ES6+)**
- **Web Crypto API** (`window.crypto.subtle`)

---

## 📦 Installation & Usage / Kurulum ve Kullanım

Since PrivaVault is built as a **single-file application** (`index.html`), no complex installation or Node.js environment is required!

PrivaVault tek bir `index.html` dosyası olarak tasarlandığı için karmaşık bir kurulum gerektirmez.

1. Clone or download the repository / Projeyi indirin:
   ```bash
   git clone [https://github.com/your-username/privavault.git](https://github.com/your-username/privavault.git)

// Reproduces PrivaVault 1.4.0 `encrypt-files` (main.js L217-308) without dialogs/UI.
// Usage: node gen.js <outFile> <password> <iterations> <input paths...>
const fs = require('fs');
const path = require('path');
const crypto = require('crypto');
const archiver = require('archiver');

async function encryptV1(filePaths, password, iterCount, outPath) {
  const salt = crypto.randomBytes(16);
  const iv = crypto.randomBytes(12);
  const key = crypto.pbkdf2Sync(password, salt, iterCount, 32, 'sha256');
  const cipher = crypto.createCipheriv('aes-256-gcm', key, iv);
  const writeStream = fs.createWriteStream(outPath);
  const iterBuffer = Buffer.alloc(4);
  iterBuffer.writeUInt32BE(iterCount, 0);
  writeStream.write(salt);
  writeStream.write(iv);
  writeStream.write(iterBuffer);
  const archive = archiver('zip', { zlib: { level: 0 } });
  await new Promise((resolve, reject) => {
    writeStream.on('finish', resolve);
    archive.on('error', reject);
    cipher.on('error', reject);
    writeStream.on('error', reject);
    archive.pipe(cipher).pipe(writeStream);
    for (const fp of filePaths) {
      if (fs.existsSync(fp)) {
        const stat = fs.statSync(fp);
        if (stat.isDirectory()) archive.directory(fp, path.basename(fp));
        else archive.file(fp, { name: path.basename(fp) });
      }
    }
    archive.finalize();
  });
  fs.appendFileSync(outPath, cipher.getAuthTag());
}

const [out, pw, iters, ...inputs] = process.argv.slice(2);
encryptV1(inputs, pw, parseInt(iters, 10), out).then(() => console.log('wrote', out));

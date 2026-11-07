// Encrypts an arbitrary pre-built ZIP exactly the way v1 does (for malicious-entry fixtures).
// Usage: node gen_zip.js <zipIn> <outFile> <password> <iterations>
const fs = require('fs');
const crypto = require('crypto');
const [zipIn, out, pw, iters] = process.argv.slice(2);
const it = parseInt(iters, 10);
const salt = crypto.randomBytes(16), iv = crypto.randomBytes(12);
const key = crypto.pbkdf2Sync(pw, salt, it, 32, 'sha256');
const c = crypto.createCipheriv('aes-256-gcm', key, iv);
const ib = Buffer.alloc(4); ib.writeUInt32BE(it, 0);
const ct = Buffer.concat([c.update(fs.readFileSync(zipIn)), c.final()]);
fs.writeFileSync(out, Buffer.concat([salt, iv, ib, ct, c.getAuthTag()]));
console.log('wrote', out);

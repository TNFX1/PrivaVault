const { app, BrowserWindow, ipcMain, dialog } = require('electron');
const path = require('path');
const fs = require('fs');
const crypto = require('crypto');
const { pipeline } = require('stream/promises');

function createWindow() {
  const win = new BrowserWindow({
    width: 520,
    height: 780,
    resizable: false,
    webPreferences: {
      preload: path.join(__dirname, 'preload.js'),
      contextIsolation: true,
      nodeIntegration: false
    }
  });

  win.setMenuBarVisibility(false);
  win.loadFile('index.html');
}

app.whenReady().then(createWindow);

app.on('window-all-closed', () => {
  if (process.platform !== 'darwin') app.quit();
});

// Dosya Seçme Diyaloğu
ipcMain.handle('select-files', async () => {
  const result = await dialog.showOpenDialog({
    properties: ['openFile', 'multiSelections']
  });
  if (result.canceled) return [];
  return result.filePaths.map(filePath => ({
    path: filePath,
    name: path.basename(filePath),
    size: fs.statSync(filePath).size
  }));
});

// Şifreli Dosya Seçme Diyaloğu (.vault / .enc)
ipcMain.handle('select-vault-file', async () => {
  const result = await dialog.showOpenDialog({
    properties: ['openFile'],
    filters: [{ name: 'Vault Files', extensions: ['vault', 'enc'] }]
  });
  if (result.canceled || result.filePaths.length === 0) return null;
  const filePath = result.filePaths[0];
  return {
    path: filePath,
    name: path.basename(filePath),
    size: fs.statSync(filePath).size
  };
});

// Şifreleme İşlemi (AES-256-GCM Akış / Stream)
ipcMain.handle('encrypt-file', async (event, { filePath, password }) => {
  try {
    const saveResult = await dialog.showSaveDialog({
      title: 'Şifreli Dosyayı Kaydet / Save Encrypted File',
      defaultPath: filePath + '.vault'
    });

    if (saveResult.canceled || !saveResult.filePath) return { success: false, status: 'canceled' };

    const outputPath = saveResult.filePath;
    const salt = crypto.randomBytes(16);
    const iv = crypto.randomBytes(12);

    // PBKDF2 ile anahtar türetme
    const key = crypto.pbkdf2Sync(password, salt, 100000, 32, 'sha256');
    const cipher = crypto.createCipheriv('aes-256-gcm', key, iv);

    const readStream = fs.createReadStream(filePath);
    const writeStream = fs.createWriteStream(outputPath);

    // Header: Salt (16 byte) + IV (12 byte)
    writeStream.write(salt);
    writeStream.write(iv);

    // Akış şifreleme
    await pipeline(readStream, cipher, writeStream);

    // Auth tag (GCM için doğrulama etiketi) en sona yazılır
    const authTag = cipher.getAuthTag();
    fs.appendFileSync(outputPath, authTag);

    return { success: true, outputPath };
  } catch (err) {
    return { success: false, error: err.message };
  }
});

// Şifre Çözme İşlemi (AES-256-GCM Akış / Stream)
ipcMain.handle('decrypt-file', async (event, { filePath, password }) => {
  try {
    const fileStats = fs.statSync(filePath);
    if (fileStats.size < 16 + 12 + 16) {
      throw new Error('Geçersiz şifreli dosya formatı.');
    }

    const saveResult = await dialog.showSaveDialog({
      title: 'Çözülen Dosyayı Kaydet / Save Decrypted File',
      defaultPath: filePath.replace(/\.vault$|\.enc$/, '')
    });

    if (saveResult.canceled || !saveResult.filePath) return { success: false, status: 'canceled' };

    const outputPath = saveResult.filePath;

    // Header okuma (Salt & IV)
    const fd = fs.openSync(filePath, 'r');
    const salt = Buffer.alloc(16);
    const iv = Buffer.alloc(12);
    const authTag = Buffer.alloc(16);

    fs.readSync(fd, salt, 0, 16, 0);
    fs.readSync(fd, iv, 0, 12, 16);
    // Auth Tag dosyanın son 16 baytındadır
    fs.readSync(fd, authTag, 0, 16, fileStats.size - 16);
    fs.closeSync(fd);

    const key = crypto.pbkdf2Sync(password, salt, 100000, 32, 'sha256');
    const decipher = crypto.createDecipheriv('aes-256-gcm', key, iv);
    decipher.setAuthTag(authTag);

    // Veri gövdesini akış ile okuma (Salt+IV sonrasından AuthTag öncesine kadar)
    const readStream = fs.createReadStream(filePath, {
      start: 28,
      end: fileStats.size - 17
    });
    const writeStream = fs.createWriteStream(outputPath);

    await pipeline(readStream, decipher, writeStream);

    return { success: true, outputPath };
  } catch (err) {
    return { success: false, error: 'Şifre hatalı veya dosya bozulmuş olabilir. (' + err.message + ')' };
  }
});
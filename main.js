const { app, BrowserWindow, ipcMain, dialog } = require('electron');
const path = require('path');
const fs = require('fs');
const crypto = require('crypto');
const JSZip = require('jszip');

let mainWindow;

function createWindow() {
  const iconPath = path.join(__dirname, 'logo.png');

  mainWindow = new BrowserWindow({
    width: 580,
    height: 880,
    resizable: false,
    icon: fs.existsSync(iconPath) ? iconPath : undefined,
    webPreferences: {
      preload: path.join(__dirname, 'preload.js'),
      contextIsolation: true,
      nodeIntegration: false
    }
  });

  mainWindow.setMenuBarVisibility(false);
  mainWindow.loadFile('index.html');
}

app.whenReady().then(createWindow);

app.on('window-all-closed', () => {
  if (process.platform !== 'darwin') app.quit();
});

// Dosya Seçme
ipcMain.handle('select-files', async () => {
  const result = await dialog.showOpenDialog(mainWindow, {
    properties: ['openFile', 'multiSelections']
  });
  if (result.canceled) return [];
  return result.filePaths.map(filePath => ({
    path: filePath,
    name: path.basename(filePath),
    size: fs.statSync(filePath).size
  }));
});

// Vault Dosyası Seçme
ipcMain.handle('select-vault-file', async () => {
  const result = await dialog.showOpenDialog(mainWindow, {
    properties: ['openFile']
  });
  if (result.canceled || result.filePaths.length === 0) return null;
  const filePath = result.filePaths[0];
  return {
    path: filePath,
    name: path.basename(filePath),
    size: fs.statSync(filePath).size
  };
});

// Şifreleme: Tüm Dosyaları Tek Bir Şifreli Pakete Dönüştürür
ipcMain.handle('encrypt-file', async (event, { filePaths, password, customExt, iterations }) => {
  try {
    const ext = customExt ? customExt.replace(/^\./, '') : 'pvault';
    const iter = iterations ? parseInt(iterations, 10) : 100000;

    const defaultOutputName = `Vault_Archive_${Date.now()}.${ext}`;
    const saveResult = await dialog.showSaveDialog(mainWindow, {
      title: 'Save Encrypted Vault Package',
      defaultPath: defaultOutputName
    });

    if (saveResult.canceled || !saveResult.filePath) return { success: false, status: 'canceled' };

    // 1. Tüm dosyaları bellek içi ZIP konteynırına ekle
    const innerZip = new JSZip();
    for (const filePath of filePaths) {
      const fileData = fs.readFileSync(filePath);
      innerZip.file(path.basename(filePath), fileData);
    }

    const zipBuffer = await innerZip.generateAsync({ type: 'nodebuffer', compression: 'DEFLATE' });

    // 2. ZIP paketini AES-256-GCM ile şifrele
    const salt = crypto.randomBytes(16);
    const iv = crypto.randomBytes(12);
    const key = crypto.pbkdf2Sync(password, salt, iter, 32, 'sha256');

    const cipher = crypto.createCipheriv('aes-256-gcm', key, iv);
    const encryptedData = Buffer.concat([cipher.update(zipBuffer), cipher.final()]);
    const authTag = cipher.getAuthTag();

    // Yapı: Salt (16B) + IV (12B) + Encrypted ZIP + AuthTag (16B)
    const finalPackage = Buffer.concat([salt, iv, encryptedData, authTag]);
    fs.writeFileSync(saveResult.filePath, finalPackage);

    return { success: true, count: filePaths.length };
  } catch (err) {
    return { success: false, error: err.message };
  }
});

// Vault Paketini Açma ve İçeriği Listeleme
ipcMain.handle('inspect-vault', async (event, { filePath, password, iterations }) => {
  try {
    const iter = iterations ? parseInt(iterations, 10) : 100000;
    const buffer = fs.readFileSync(filePath);

    if (buffer.length < 16 + 12 + 16) {
      throw new Error('Invalid vault file structure.');
    }

    const salt = buffer.subarray(0, 16);
    const iv = buffer.subarray(16, 28);
    const authTag = buffer.subarray(buffer.length - 16);
    const encryptedData = buffer.subarray(28, buffer.length - 16);

    const key = crypto.pbkdf2Sync(password, salt, iter, 32, 'sha256');
    const decipher = crypto.createDecipheriv('aes-256-gcm', key, iv);
    decipher.setAuthTag(authTag);

    const decryptedZipBuffer = Buffer.concat([decipher.update(encryptedData), decipher.final()]);
    const zipArchive = await JSZip.loadAsync(decryptedZipBuffer);

    const files = [];
    zipArchive.forEach((relativePath, file) => {
      if (!file.dir) {
        files.push({ name: relativePath, size: file._data.uncompressedSize || 0 });
      }
    });

    return { success: true, files };
  } catch (err) {
    return { success: false, error: 'Incorrect password or corrupted vault file.' };
  }
});

// Tekil Dosya Çıkarma
ipcMain.handle('extract-single-file', async (event, { filePath, password, fileName, iterations }) => {
  try {
    const iter = iterations ? parseInt(iterations, 10) : 100000;
    const buffer = fs.readFileSync(filePath);

    const salt = buffer.subarray(0, 16);
    const iv = buffer.subarray(16, 28);
    const authTag = buffer.subarray(buffer.length - 16);
    const encryptedData = buffer.subarray(28, buffer.length - 16);

    const key = crypto.pbkdf2Sync(password, salt, iter, 32, 'sha256');
    const decipher = crypto.createDecipheriv('aes-256-gcm', key, iv);
    decipher.setAuthTag(authTag);

    const decryptedZipBuffer = Buffer.concat([decipher.update(encryptedData), decipher.final()]);
    const zipArchive = await JSZip.loadAsync(decryptedZipBuffer);

    const saveResult = await dialog.showSaveDialog(mainWindow, {
      title: 'Save File',
      defaultPath: fileName
    });

    if (saveResult.canceled || !saveResult.filePath) return { success: false, status: 'canceled' };

    const fileBuffer = await zipArchive.file(fileName).async('nodebuffer');
    fs.writeFileSync(saveResult.filePath, fileBuffer);

    return { success: true };
  } catch (err) {
    return { success: false, error: err.message };
  }
});

// Tüm Dosyaları Çıkarma (Klasör veya ZIP Olarak)
ipcMain.handle('extract-all-files', async (event, { filePath, password, mode, iterations }) => {
  try {
    const iter = iterations ? parseInt(iterations, 10) : 100000;
    const buffer = fs.readFileSync(filePath);

    const salt = buffer.subarray(0, 16);
    const iv = buffer.subarray(16, 28);
    const authTag = buffer.subarray(buffer.length - 16);
    const encryptedData = buffer.subarray(28, buffer.length - 16);

    const key = crypto.pbkdf2Sync(password, salt, iter, 32, 'sha256');
    const decipher = crypto.createDecipheriv('aes-256-gcm', key, iv);
    decipher.setAuthTag(authTag);

    const decryptedZipBuffer = Buffer.concat([decipher.update(encryptedData), decipher.final()]);

    if (mode === 'zip') {
      const saveResult = await dialog.showSaveDialog(mainWindow, {
        title: 'Save Decrypted Package as ZIP',
        defaultPath: 'Decrypted_Archive.zip'
      });
      if (saveResult.canceled || !saveResult.filePath) return { success: false, status: 'canceled' };

      fs.writeFileSync(saveResult.filePath, decryptedZipBuffer);
    } else {
      const folderResult = await dialog.showOpenDialog(mainWindow, {
        title: 'Select Destination Folder',
        properties: ['openDirectory']
      });
      if (folderResult.canceled || folderResult.filePaths.length === 0) return { success: false, status: 'canceled' };

      const targetFolder = folderResult.filePaths[0];
      const zipArchive = await JSZip.loadAsync(decryptedZipBuffer);

      for (const relativePath of Object.keys(zipArchive.files)) {
        const file = zipArchive.files[relativePath];
        if (!file.dir) {
          const content = await file.async('nodebuffer');
          fs.writeFileSync(path.join(targetFolder, relativePath), content);
        }
      }
    }

    return { success: true };
  } catch (err) {
    return { success: false, error: err.message };
  }
});
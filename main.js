const { app, BrowserWindow, ipcMain, dialog, Notification, Tray, Menu, nativeImage } = require('electron');
const path = require('path');
const fs = require('fs');
const crypto = require('crypto');
const https = require('https');
const StreamZip = require('node-stream-zip');
const archiver = require('archiver');

let mainWindow;
let tray = null;
let currentLanguage = 'en';
let fileToProcessOnLaunch = null;

if (process.platform === 'win32' && process.argv.length >= 2) {
  const possibleFile = process.argv[1];
  if (possibleFile && possibleFile !== '.' && !possibleFile.includes('node_modules') && fs.existsSync(possibleFile)) {
    fileToProcessOnLaunch = possibleFile;
  }
}

function getAppIcon() {
  const pngPath = path.join(__dirname, 'assets', 'logo.png');
  const icoPath = path.join(__dirname, 'assets', 'logo.ico');

  let windowImg = null;
  let trayImg = null;

  if (fs.existsSync(pngPath)) {
    try {
      windowImg = nativeImage.createFromPath(pngPath);
      // Tray (görev tepsisi) ikonunu biraz daha optimize boyutta oluşturuyoruz
       trayImg = windowImg.resize({ width: 48, height: 48, quality: 'best' });
    } catch (e) {
      console.error('PNG icon loading error:', e);
    }
  }

  if ((!windowImg || windowImg.isEmpty()) && fs.existsSync(icoPath)) {
    try {
      windowImg = nativeImage.createFromPath(icoPath);
      trayImg = windowImg.resize({ width: 48, height: 48, quality: 'best' });
    } catch (e) {
      console.error('ICO icon loading error:', e);
    }
  }

  return {
    windowImg: windowImg && !windowImg.isEmpty() ? windowImg : undefined,
    trayImg: trayImg && !trayImg.isEmpty() ? trayImg : undefined
  };
}
function updateTrayMenu(lang) {
  if (!tray) return;
  currentLanguage = lang || currentLanguage;
  const isTr = currentLanguage.startsWith('tr');

  const contextMenu = Menu.buildFromTemplate([
    { 
      label: isTr ? 'PrivaVault Göster' : 'Show PrivaVault', 
      click: () => mainWindow.show() 
    },
    { type: 'separator' },
    { 
      label: isTr ? 'Çıkış' : 'Quit', 
      click: () => app.quit() 
    }
  ]);
  tray.setToolTip('PrivaVault - Secure Encryption');
  tray.setContextMenu(contextMenu);
}

function createWindow() {
  const { windowImg, trayImg } = getAppIcon();
  const sysLocale = app.getLocale() || 'en';
  currentLanguage = sysLocale.toLowerCase().startsWith('tr') ? 'tr' : 'en';

  mainWindow = new BrowserWindow({
    width: 620,
    height: 940,
    resizable: false,
    icon: windowImg, // Pencere ve görev çubuğu için PNG aktif
    webPreferences: {
      preload: path.join(__dirname, 'preload.js'),
      contextIsolation: true,
      nodeIntegration: false
    }
  });

  mainWindow.setMenuBarVisibility(false);
  mainWindow.loadFile('index.html');

  if (trayImg) {
    try {
      tray = new Tray(trayImg);
      updateTrayMenu(currentLanguage);
      tray.on('double-click', () => mainWindow.show());
    } catch (err) {
      console.error('Tray creation failed:', err);
    }
  }

  mainWindow.on('close', (event) => {
    if (!app.isQuitting) {
      event.preventDefault();
      mainWindow.hide();
    }
  });

  mainWindow.webContents.on('did-finish-load', () => {
    checkForUpdates();
  });
}

app.on('before-quit', () => {
  app.isQuitting = true;
});

app.whenReady().then(createWindow);

app.on('window-all-closed', () => {
  if (process.platform !== 'darwin') app.quit();
});

function checkForUpdates() {
  const currentVersion = app.getVersion() || '1.4.0';
  const options = {
    hostname: 'api.github.com',
    path: '/repos/TNFX1/PrivaVault/releases/latest',
    headers: { 'User-Agent': 'PrivaVault-App' }
  };

  https.get(options, (res) => {
    let data = '';
    res.on('data', chunk => data += chunk);
    res.on('end', () => {
      try {
        const json = JSON.parse(data);
        if (json.tag_name) {
          const latestVersion = json.tag_name.replace('v', '');
          if (isNewerVersion(currentVersion, latestVersion)) {
            if (mainWindow && !mainWindow.isDestroyed()) {
              mainWindow.webContents.send('update-available', {
                version: json.tag_name,
                url: json.html_url
              });
            }
          }
        }
      } catch (e) {}
    });
  }).on('error', () => {});
}

function isNewerVersion(current, latest) {
  const c = current.split('.').map(Number);
  const l = latest.split('.').map(Number);
  for (let i = 0; i < Math.max(c.length, l.length); i++) {
    const cv = c[i] || 0;
    const lv = l[i] || 0;
    if (lv > cv) return true;
    if (lv < cv) return false;
  }
  return false;
}

function sendProgress(percent, status, bytesProcessed = 0, totalBytes = 0, speedBps = 0, etaSec = 0) {
  if (mainWindow && !mainWindow.isDestroyed()) {
    mainWindow.webContents.send('progress-update', { 
      percent, 
      status, 
      bytesProcessed, 
      totalBytes, 
      speedBps, 
      etaSec 
    });
    
    if (percent >= 100 || percent < 0) {
      mainWindow.setProgressBar(-1);
    } else {
      mainWindow.setProgressBar(Math.max(0, percent / 100));
    }
  }
}

ipcMain.handle('get-app-locale', () => app.getLocale());
ipcMain.handle('get-launched-file', () => fileToProcessOnLaunch);

ipcMain.handle('set-language', (event, lang) => {
  updateTrayMenu(lang);
  return true;
});

ipcMain.handle('select-files', async () => {
  const result = await dialog.showOpenDialog(mainWindow, { properties: ['openFile', 'multiSelections'] });
  return result.canceled ? [] : result.filePaths;
});

ipcMain.handle('select-folders', async () => {
  const result = await dialog.showOpenDialog(mainWindow, { properties: ['openDirectory', 'multiSelections'] });
  return result.canceled ? [] : result.filePaths;
});

ipcMain.handle('select-vault-file', async () => {
  const result = await dialog.showOpenDialog(mainWindow, {
    filters: [{ name: 'PrivaVault Archive', extensions: ['pvault', 'enc', 'bin', 'vault', '*'] }],
    properties: ['openFile']
  });
  return (result.canceled || result.filePaths.length === 0) ? null : result.filePaths[0];
});

function getTotalSize(paths) {
  let total = 0;
  for (const p of paths) {
    if (fs.existsSync(p)) {
      const stat = fs.statSync(p);
      if (stat.isDirectory()) {
        const files = fs.readdirSync(p);
        total += getTotalSize(files.map(f => path.join(p, f)));
      } else {
        total += stat.size;
      }
    }
  }
  return total;
}

ipcMain.handle('encrypt-files', async (event, { filePaths, password, iterations, extension }) => {
  let key = null;
  try {
    const ext = extension ? extension.replace('.', '') : 'pvault';
    const defaultName = filePaths.length === 1 
      ? `${path.basename(filePaths[0], path.extname(filePaths[0]))}.${ext}`
      : `vault_${Date.now()}.${ext}`;

    const saveResult = await dialog.showSaveDialog(mainWindow, {
      title: 'Save Encrypted Vault',
      defaultPath: defaultName,
      filters: [{ name: 'Encrypted Vault', extensions: [ext] }]
    });

    if (saveResult.canceled || !saveResult.filePath) {
      return { success: false, status: 'canceled' };
    }

    sendProgress(5, 'Deriving encryption key...');
    const salt = crypto.randomBytes(16);
    const iv = crypto.randomBytes(12);
    const iterCount = parseInt(iterations, 10) || 100000;

    key = crypto.pbkdf2Sync(password, salt, iterCount, 32, 'sha256');

    const totalBytes = getTotalSize(filePaths);
    let bytesProcessed = 0;
    const startTime = Date.now();

    const cipher = crypto.createCipheriv('aes-256-gcm', key, iv);
    const writeStream = fs.createWriteStream(saveResult.filePath);

    const iterBuffer = Buffer.alloc(4);
    iterBuffer.writeUInt32BE(iterCount, 0);

    writeStream.write(salt);
    writeStream.write(iv);
    writeStream.write(iterBuffer);

    const archive = archiver('zip', { zlib: { level: 0 } });

    cipher.on('data', (chunk) => {
      bytesProcessed += chunk.length;
      const elapsedTime = (Date.now() - startTime) / 1000;
      const speedBps = elapsedTime > 0 ? bytesProcessed / elapsedTime : 0;
      const remainingBytes = Math.max(0, totalBytes - bytesProcessed);
      const etaSec = speedBps > 0 ? Math.ceil(remainingBytes / speedBps) : 0;
      const pct = Math.min(99, Math.round((bytesProcessed / (totalBytes || 1)) * 100));

      sendProgress(pct, 'Encrypting stream...', bytesProcessed, totalBytes, speedBps, etaSec);
    });

    await new Promise((resolve, reject) => {
      writeStream.on('finish', resolve);
      archive.on('error', reject);
      cipher.on('error', reject);
      writeStream.on('error', reject);

      archive.pipe(cipher).pipe(writeStream);

      for (const fp of filePaths) {
        if (fs.existsSync(fp)) {
          const stat = fs.statSync(fp);
          if (stat.isDirectory()) {
            archive.directory(fp, path.basename(fp));
          } else {
            archive.file(fp, { name: path.basename(fp) });
          }
        }
      }

      archive.finalize();
    });

    const authTag = cipher.getAuthTag();
    fs.appendFileSync(saveResult.filePath, authTag);

    sendProgress(100, 'Encryption Complete!', totalBytes, totalBytes, 0, 0);

    if (Notification.isSupported()) {
      new Notification({ title: 'PrivaVault', body: 'File(s) encrypted successfully!' }).show();
    }

    return { success: true };

  } catch (err) {
    sendProgress(-1, 'Error');
    return { success: false, error: err.message };
  } finally {
    if (Buffer.isBuffer(key)) key.fill(0);
  }
});

async function verifyVaultPasswordFast(filePath, password) {
  const fd = fs.openSync(filePath, 'r');
  const fileSize = fs.statSync(filePath).size;

  if (fileSize < 48) {
    fs.closeSync(fd);
    throw new Error('INVALID_PASSWORD');
  }

  const salt = Buffer.alloc(16);
  const iv = Buffer.alloc(12);
  const iterBuffer = Buffer.alloc(4);

  fs.readSync(fd, salt, 0, 16, 0);
  fs.readSync(fd, iv, 0, 12, 16);
  fs.readSync(fd, iterBuffer, 0, 4, 28);
  fs.closeSync(fd);

  const iterations = iterBuffer.readUInt32BE(0);
  const key = crypto.pbkdf2Sync(password, salt, iterations, 32, 'sha256');

  const decipher = crypto.createDecipheriv('aes-256-gcm', key, iv);
  
  const chunkSize = Math.min(1024, fileSize - 48);
  const chunkBuf = Buffer.alloc(chunkSize);
  
  const fdChunk = fs.openSync(filePath, 'r');
  fs.readSync(fdChunk, chunkBuf, 0, chunkSize, 32);
  fs.closeSync(fdChunk);

  try {
    const decryptedHeader = decipher.update(chunkBuf);
    if (decryptedHeader.length < 4 || decryptedHeader[0] !== 0x50 || decryptedHeader[1] !== 0x4b) {
      if (Buffer.isBuffer(key)) key.fill(0);
      throw new Error('INVALID_PASSWORD');
    }
  } catch (e) {
    if (Buffer.isBuffer(key)) key.fill(0);
    throw new Error('INVALID_PASSWORD');
  }

  if (Buffer.isBuffer(key)) key.fill(0);
  return true;
}

async function decryptToTempZip(filePath, password) {
  const fd = fs.openSync(filePath, 'r');
  const fileSize = fs.statSync(filePath).size;

  const salt = Buffer.alloc(16);
  const iv = Buffer.alloc(12);
  const iterBuffer = Buffer.alloc(4);
  const authTag = Buffer.alloc(16);

  fs.readSync(fd, salt, 0, 16, 0);
  fs.readSync(fd, iv, 0, 12, 16);
  fs.readSync(fd, iterBuffer, 0, 4, 28);
  fs.readSync(fd, authTag, 0, 16, fileSize - 16);
  fs.closeSync(fd);

  const iterations = iterBuffer.readUInt32BE(0);
  const key = crypto.pbkdf2Sync(password, salt, iterations, 32, 'sha256');

  const decipher = crypto.createDecipheriv('aes-256-gcm', key, iv);
  decipher.setAuthTag(authTag);

  const tempZipPath = path.join(app.getPath('temp'), `pvault_dec_${Date.now()}.zip`);
  const totalEncryptedBytes = fileSize - 48;
  let bytesRead = 0;
  const startTime = Date.now();

  const readStream = fs.createReadStream(filePath, { start: 32, end: fileSize - 17 });
  const writeStream = fs.createWriteStream(tempZipPath);

  readStream.on('data', (chunk) => {
    bytesRead += chunk.length;
    const elapsedTime = (Date.now() - startTime) / 1000;
    const speedBps = elapsedTime > 0 ? bytesRead / elapsedTime : 0;
    const remainingBytes = Math.max(0, totalEncryptedBytes - bytesRead);
    const etaSec = speedBps > 0 ? Math.ceil(remainingBytes / speedBps) : 0;
    
    const pct = Math.min(80, Math.max(10, Math.round(10 + (bytesRead / (totalEncryptedBytes || 1)) * 70)));
    sendProgress(pct, 'Decrypting stream...', bytesRead, totalEncryptedBytes, speedBps, etaSec);
  });

  try {
    await new Promise((resolve, reject) => {
      readStream.pipe(decipher).pipe(writeStream);
      writeStream.on('finish', resolve);
      readStream.on('error', reject);
      decipher.on('error', reject);
      writeStream.on('error', reject);
    });
    return tempZipPath;
  } catch (err) {
    if (fs.existsSync(tempZipPath)) {
      try { fs.unlinkSync(tempZipPath); } catch(e) {}
    }
    throw new Error('INVALID_PASSWORD');
  } finally {
    if (Buffer.isBuffer(key)) key.fill(0);
  }
}

ipcMain.handle('inspect-vault', async (event, { filePath, password }) => {
  let tempZipPath = null;
  try {
    sendProgress(5, 'Verifying password...');
    await verifyVaultPasswordFast(filePath, password);

    sendProgress(20, 'Decrypting stream...');
    tempZipPath = await decryptToTempZip(filePath, password);

    sendProgress(85, 'Reading file structure...');
    const zip = new StreamZip.async({ file: tempZipPath });
    const entries = await zip.entries();
    await zip.close();

    const fileList = Object.values(entries).map(e => ({
      name: e.name,
      size: e.size,
      isDirectory: e.isDirectory
    }));

    if (fs.existsSync(tempZipPath)) fs.unlinkSync(tempZipPath);
    sendProgress(100, 'Inspection complete.');
    return { success: true, entries: fileList };

  } catch (err) {
    if (tempZipPath && fs.existsSync(tempZipPath)) {
      try { fs.unlinkSync(tempZipPath); } catch(e) {}
    }
    sendProgress(-1, 'Failed');
    return { 
      success: false, 
      error: currentLanguage.startsWith('tr') ? 'Yanlış Şifre!' : 'Incorrect Password!' 
    };
  }
});

ipcMain.handle('extract-single-file', async (event, { filePath, password, entryName }) => {
  let tempZipPath = null;
  try {
    await verifyVaultPasswordFast(filePath, password);

    const saveResult = await dialog.showSaveDialog(mainWindow, {
      title: 'Save Extracted File',
      defaultPath: path.basename(entryName)
    });
    if (saveResult.canceled || !saveResult.filePath) return { success: false, status: 'canceled' };

    sendProgress(10, 'Decrypting archive...');
    tempZipPath = await decryptToTempZip(filePath, password);

    sendProgress(85, 'Extracting selected file...');
    const zip = new StreamZip.async({ file: tempZipPath });
    await zip.extract(entryName, saveResult.filePath);
    await zip.close();

    if (fs.existsSync(tempZipPath)) fs.unlinkSync(tempZipPath);
    sendProgress(100, 'File extracted successfully!');
    return { success: true };

  } catch (err) {
    if (tempZipPath && fs.existsSync(tempZipPath)) {
      try { fs.unlinkSync(tempZipPath); } catch(e) {}
    }
    sendProgress(-1, 'Failed');
    return { 
      success: false, 
      error: currentLanguage.startsWith('tr') ? 'Yanlış Şifre!' : 'Incorrect Password!' 
    };
  }
});

ipcMain.handle('decrypt-vault', async (event, { filePath, password, mode }) => {
  let tempZipPath = null;
  try {
    sendProgress(5, 'Verifying password...');
    await verifyVaultPasswordFast(filePath, password);

    if (mode === 'single') {
      const saveResult = await dialog.showSaveDialog(mainWindow, {
        title: 'Save Decrypted File',
        defaultPath: 'decrypted_file'
      });

      if (saveResult.canceled || !saveResult.filePath) {
        sendProgress(-1, 'Canceled');
        return { success: false, status: 'canceled' };
      }

      sendProgress(20, 'Decrypting archive...');
      tempZipPath = await decryptToTempZip(filePath, password);

      sendProgress(60, 'Reading archive entries...');
      const zip = new StreamZip.async({ file: tempZipPath });
      const entries = await zip.entries();
      const entryKeys = Object.keys(entries);

      if (entryKeys.length === 0) {
        await zip.close();
        throw new Error('Vault is empty.');
      }

      const firstEntry = entryKeys[0];

      sendProgress(80, 'Extracting file...');
      await zip.extract(firstEntry, saveResult.filePath);
      await zip.close();

    } else if (mode === 'zip') {
      const saveResult = await dialog.showSaveDialog(mainWindow, {
        title: 'Save Decrypted ZIP',
        defaultPath: `Decrypted_${new Date().toISOString().slice(0, 10)}.zip`,
        filters: [{ name: 'ZIP Archive', extensions: ['zip'] }]
      });

      if (saveResult.canceled || !saveResult.filePath) {
        sendProgress(-1, 'Canceled');
        return { success: false, status: 'canceled' };
      }

      sendProgress(30, 'Decrypting archive...');
      tempZipPath = await decryptToTempZip(filePath, password);

      sendProgress(70, 'Saving ZIP file...');
      fs.copyFileSync(tempZipPath, saveResult.filePath);

    } else {
      const folderResult = await dialog.showOpenDialog(mainWindow, {
        title: 'Select Destination Folder',
        properties: ['openDirectory']
      });

      if (folderResult.canceled || folderResult.filePaths.length === 0) {
        sendProgress(-1, 'Canceled');
        return { success: false, status: 'canceled' };
      }

      sendProgress(30, 'Decrypting stream...');
      tempZipPath = await decryptToTempZip(filePath, password);

      sendProgress(70, 'Extracting files...');
      const targetFolder = folderResult.filePaths[0];
      const zip = new StreamZip.async({ file: tempZipPath });
      await zip.extract(null, targetFolder);
      await zip.close();
    }

    if (tempZipPath && fs.existsSync(tempZipPath)) {
      fs.unlinkSync(tempZipPath);
    }

    sendProgress(100, 'Decryption complete!');

    if (Notification.isSupported()) {
      new Notification({ title: 'PrivaVault', body: 'Decryption completed successfully!' }).show();
    }

    return { success: true };

  } catch (err) {
    if (tempZipPath && fs.existsSync(tempZipPath)) {
      try { fs.unlinkSync(tempZipPath); } catch(e) {}
    }
    sendProgress(-1, 'Failed');
    return { success: false, error: currentLanguage.startsWith('tr') ? 'Şifre çözme başarısız veya şifre yanlış!' : 'Decryption failed. Password may be wrong or file is corrupted.' };
  }
});
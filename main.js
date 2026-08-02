const { app, BrowserWindow, ipcMain, dialog } = require('electron');
const path = require('path');
const fs = require('fs');
const crypto = require('crypto');
const StreamZip = require('node-stream-zip');

// ESM Archiver modülünü güvenli şekilde fonksiyon olarak yüklemek için yapılandırma
let archiverModule = null;
async function getArchiver() {
  if (!archiverModule) {
    const mod = await import('archiver');
    // modülün kendisi fonksiyon mu yoksa default export mu kontrol edelim
    archiverModule = typeof mod === 'function' ? mod : (mod.default || mod);
  }
  return archiverModule;
}

let mainWindow;

function createWindow() {
  // Logonun doğru klasörde (assets/logo.png veya kök dizinde) aranması
  const iconPathPng = path.join(__dirname, 'assets', 'logo.png');
  const iconPathRoot = path.join(__dirname, 'logo.png');
  const iconPath = fs.existsSync(iconPathPng) ? iconPathPng : (fs.existsSync(iconPathRoot) ? iconPathRoot : undefined);

  mainWindow = new BrowserWindow({
    width: 580,
    height: 880,
    resizable: false,
    icon: iconPath,
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

function sendProgress(percent, status) {
  if (mainWindow && !mainWindow.isDestroyed()) {
    mainWindow.webContents.send('update-progress', { percent, status });
  }
}

// Gelişmiş Header Okuyucu: Iterations bilgisi de dahil edildi
function getVaultHeader(filePath) {
  const stat = fs.statSync(filePath);
  const fileSize = stat.size;

  // Salt(16) + IV(12) + Iterations(4) + AuthTag(16) = minimum 48 Byte
  if (fileSize < 48) {
    throw new Error('Invalid vault file structure.');
  }

  const fd = fs.openSync(filePath, 'r');
  const salt = Buffer.alloc(16);
  const iv = Buffer.alloc(12);
  const iterBuf = Buffer.alloc(4);
  const authTag = Buffer.alloc(16);

  fs.readSync(fd, salt, 0, 16, 0);
  fs.readSync(fd, iv, 0, 12, 16);
  fs.readSync(fd, iterBuf, 0, 4, 28);
  fs.readSync(fd, authTag, 0, 16, fileSize - 16);
  fs.closeSync(fd);

  const iterations = iterBuf.readUInt32BE(0);

  return { fileSize, salt, iv, iterations, authTag };
}

async function decryptToTempZip(filePath, password) {
  const { fileSize, salt, iv, iterations, authTag } = getVaultHeader(filePath);
  const key = crypto.pbkdf2Sync(password, salt, iterations, 32, 'sha256');

  const decipher = crypto.createDecipheriv('aes-256-gcm', key, iv);
  decipher.setAuthTag(authTag);

  const tempZipPath = path.join(app.getPath('temp'), `pv_temp_${Date.now()}.zip`);
  // Veri başlangıcı: 16 (salt) + 12 (iv) + 4 (iter) = 32. Byte
  const readStream = fs.createReadStream(filePath, { start: 32, end: fileSize - 17 });
  const writeStream = fs.createWriteStream(tempZipPath);

  await new Promise((resolve, reject) => {
    readStream.pipe(decipher).pipe(writeStream);
    writeStream.on('finish', resolve);
    readStream.on('error', reject);
    decipher.on('error', reject);
    writeStream.on('error', reject);
  });

  return tempZipPath;
}

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

    sendProgress(5, 'Initializing stream encryption...');

    const salt = crypto.randomBytes(16);
    const iv = crypto.randomBytes(12);
    const key = crypto.pbkdf2Sync(password, salt, iter, 32, 'sha256');

    const iterBuffer = Buffer.alloc(4);
    iterBuffer.writeUInt32BE(iter, 0);

    const cipher = crypto.createCipheriv('aes-256-gcm', key, iv);
    const outputStream = fs.createWriteStream(saveResult.filePath);

    // Header Yazımı: Salt (16B) + IV (12B) + Iterations (4B)
    outputStream.write(salt);
    outputStream.write(iv);
    outputStream.write(iterBuffer);

    // Dinamik Archiver Yüklemesi ve Fonksiyon Garantisi
    const archiver = await getArchiver();
    if (typeof archiver !== 'function') {
      throw new Error('Archiver module failed to load as a valid function.');
    }
    const archive = archiver('zip', { zlib: { level: 1 } });

    let processedFiles = 0;
    const totalFiles = filePaths.length;

    archive.on('progress', (data) => {
      processedFiles = data.entries.processed;
      const percent = Math.min(90, Math.round(10 + (processedFiles / totalFiles) * 80));
      sendProgress(percent, `Encrypting: ${processedFiles}/${totalFiles} files`);
    });

    return new Promise((resolve) => {
      outputStream.on('close', () => {
        const authTag = cipher.getAuthTag();
        fs.appendFileSync(saveResult.filePath, authTag);
        sendProgress(100, 'Complete');
        resolve({ success: true, count: filePaths.length });
      });

      archive.on('error', (err) => {
        resolve({ success: false, error: err.message });
      });

      archive.pipe(cipher).pipe(outputStream, { end: false });

      for (const filePath of filePaths) {
        if (fs.existsSync(filePath)) {
          archive.file(filePath, { name: path.basename(filePath) });
        }
      }

      cipher.on('end', () => outputStream.end());
      archive.finalize();
    });

  } catch (err) {
    return { success: false, error: err.message };
  }
});

ipcMain.handle('inspect-vault', async (event, { filePath, password }) => {
  let tempZipPath = null;
  try {
    sendProgress(10, 'Decrypting archive stream...');
    tempZipPath = await decryptToTempZip(filePath, password);

    sendProgress(70, 'Reading archive directory...');
    const zip = new StreamZip.async({ file: tempZipPath });
    const entries = await zip.entries();
    
    const files = [];
    for (const entry of Object.values(entries)) {
      if (!entry.isDirectory) {
        files.push({ name: entry.name, size: entry.size });
      }
    }

    await zip.close();
    if (fs.existsSync(tempZipPath)) fs.unlinkSync(tempZipPath);

    sendProgress(100, 'Done');
    return { success: true, files };
  } catch (err) {
    if (tempZipPath && fs.existsSync(tempZipPath)) fs.unlinkSync(tempZipPath);
    return { success: false, error: 'Incorrect password or corrupted vault file.' };
  }
});

ipcMain.handle('extract-single-file', async (event, { filePath, password, fileName }) => {
  let tempZipPath = null;
  try {
    sendProgress(20, 'Decrypting archive...');
    tempZipPath = await decryptToTempZip(filePath, password);

    const saveResult = await dialog.showSaveDialog(mainWindow, {
      title: 'Save File',
      defaultPath: fileName
    });

    if (saveResult.canceled || !saveResult.filePath) {
      if (fs.existsSync(tempZipPath)) fs.unlinkSync(tempZipPath);
      return { success: false, status: 'canceled' };
    }

    sendProgress(60, 'Extracting file...');
    const zip = new StreamZip.async({ file: tempZipPath });
    await zip.extract(fileName, saveResult.filePath);
    await zip.close();

    if (fs.existsSync(tempZipPath)) fs.unlinkSync(tempZipPath);

    sendProgress(100, 'Done');
    return { success: true };
  } catch (err) {
    if (tempZipPath && fs.existsSync(tempZipPath)) fs.unlinkSync(tempZipPath);
    return { success: false, error: err.message };
  }
});

ipcMain.handle('extract-all-files', async (event, { filePath, password, mode }) => {
  let tempZipPath = null;
  try {
    sendProgress(10, 'Decrypting vault stream...');

    if (mode === 'zip') {
      const saveResult = await dialog.showSaveDialog(mainWindow, {
        title: 'Save Decrypted Package as ZIP',
        defaultPath: 'Decrypted_Archive.zip'
      });
      if (saveResult.canceled || !saveResult.filePath) return { success: false, status: 'canceled' };

      const { fileSize, salt, iv, iterations, authTag } = getVaultHeader(filePath);
      const key = crypto.pbkdf2Sync(password, salt, iterations, 32, 'sha256');

      const decipher = crypto.createDecipheriv('aes-256-gcm', key, iv);
      decipher.setAuthTag(authTag);

      const readStream = fs.createReadStream(filePath, { start: 32, end: fileSize - 17 });
      const writeStream = fs.createWriteStream(saveResult.filePath);

      await new Promise((resolve, reject) => {
        readStream.pipe(decipher).pipe(writeStream);
        writeStream.on('finish', resolve);
        readStream.on('error', reject);
        decipher.on('error', reject);
        writeStream.on('error', reject);
      });
    } else {
      const folderResult = await dialog.showOpenDialog(mainWindow, {
        title: 'Select Destination Folder',
        properties: ['openDirectory']
      });
      if (folderResult.canceled || folderResult.filePaths.length === 0) return { success: false, status: 'canceled' };

      tempZipPath = await decryptToTempZip(filePath, password);
      sendProgress(60, 'Extracting files to folder...');

      const targetFolder = folderResult.filePaths[0];
      const zip = new StreamZip.async({ file: tempZipPath });
      await zip.extract(null, targetFolder);
      await zip.close();

      if (fs.existsSync(tempZipPath)) fs.unlinkSync(tempZipPath);
    }

    sendProgress(100, 'Complete');
    return { success: true };
  } catch (err) {
    if (tempZipPath && fs.existsSync(tempZipPath)) fs.unlinkSync(tempZipPath);
    return { success: false, error: err.message };
  }
});
const { app, BrowserWindow, ipcMain, dialog } = require('electron');
const path = require('path');
const fs = require('fs');
const crypto = require('crypto');
const { pipeline } = require('stream/promises');

let mainWindow;

function createWindow() {
  const iconPath = path.join(__dirname, 'logo.png');

  mainWindow = new BrowserWindow({
    width: 540,
    height: 860,
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

// Dosya Seçimi (Çoklu Dosya Desteği)
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

// Vault Dosyaları Seçimi (Çoklu Seçim Desteği)
ipcMain.handle('select-vault-file', async () => {
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

// Şifreleme İşlemi (Çoklu Dosya Döngüsü)
ipcMain.handle('encrypt-file', async (event, { filePaths, password, customExt, iterations }) => {
  try {
    const ext = customExt ? customExt.replace(/^\./, '') : 'pvault';
    const iter = iterations ? parseInt(iterations, 10) : 100000;

    // Tek dosya seçilmişse kaydetme penceresi aç, çoklu ise klasör seçtir
    let targetFolder = null;
    let singleOutputPath = null;

    if (filePaths.length === 1) {
      const defaultOutputName = `${filePaths[0]}.${ext}`;
      const saveResult = await dialog.showSaveDialog(mainWindow, {
        title: 'Save Encrypted File',
        defaultPath: defaultOutputName
      });
      if (saveResult.canceled || !saveResult.filePath) return { success: false, status: 'canceled' };
      singleOutputPath = saveResult.filePath;
    } else {
      const folderResult = await dialog.showOpenDialog(mainWindow, {
        title: 'Select Destination Folder for Encrypted Files',
        properties: ['openDirectory']
      });
      if (folderResult.canceled || folderResult.filePaths.length === 0) return { success: false, status: 'canceled' };
      targetFolder = folderResult.filePaths[0];
    }

    for (let i = 0; i < filePaths.length; i++) {
      const filePath = filePaths[i];
      const outputPath = singleOutputPath ? singleOutputPath : path.join(targetFolder, `${path.basename(filePath)}.${ext}`);

      const salt = crypto.randomBytes(16);
      const iv = crypto.randomBytes(12);

      const key = crypto.pbkdf2Sync(password, salt, iter, 32, 'sha256');
      const cipher = crypto.createCipheriv('aes-256-gcm', key, iv);

      const readStream = fs.createReadStream(filePath);
      const writeStream = fs.createWriteStream(outputPath);

      writeStream.write(salt);
      writeStream.write(iv);

      await pipeline(readStream, cipher, writeStream);

      const authTag = cipher.getAuthTag();
      fs.appendFileSync(outputPath, authTag);
    }

    return { success: true, count: filePaths.length };
  } catch (err) {
    return { success: false, error: err.message };
  }
});

// Şifre Çözme İşlemi (Çoklu Dosya Döngüsü)
ipcMain.handle('decrypt-file', async (event, { filePaths, password, iterations }) => {
  try {
    const iter = iterations ? parseInt(iterations, 10) : 100000;

    let targetFolder = null;
    let singleOutputPath = null;

    if (filePaths.length === 1) {
      const defaultOutput = filePaths[0].replace(/\.[^/.]+$/, "");
      const saveResult = await dialog.showSaveDialog(mainWindow, {
        title: 'Save Decrypted File',
        defaultPath: defaultOutput
      });
      if (saveResult.canceled || !saveResult.filePath) return { success: false, status: 'canceled' };
      singleOutputPath = saveResult.filePath;
    } else {
      const folderResult = await dialog.showOpenDialog(mainWindow, {
        title: 'Select Destination Folder for Decrypted Files',
        properties: ['openDirectory']
      });
      if (folderResult.canceled || folderResult.filePaths.length === 0) return { success: false, status: 'canceled' };
      targetFolder = folderResult.filePaths[0];
    }

    for (let i = 0; i < filePaths.length; i++) {
      const filePath = filePaths[i];
      const fileStats = fs.statSync(filePath);
      if (fileStats.size < 16 + 12 + 16) {
        throw new Error(`Invalid file structure: ${path.basename(filePath)}`);
      }

      const cleanName = path.basename(filePath).replace(/\.[^/.]+$/, "");
      const outputPath = singleOutputPath ? singleOutputPath : path.join(targetFolder, cleanName);

      const fd = fs.openSync(filePath, 'r');
      const salt = Buffer.alloc(16);
      const iv = Buffer.alloc(12);
      const authTag = Buffer.alloc(16);

      fs.readSync(fd, salt, 0, 16, 0);
      fs.readSync(fd, iv, 0, 12, 16);
      fs.readSync(fd, authTag, 0, 16, fileStats.size - 16);
      fs.closeSync(fd);

      const key = crypto.pbkdf2Sync(password, salt, iter, 32, 'sha256');
      const decipher = crypto.createDecipheriv('aes-256-gcm', key, iv);
      decipher.setAuthTag(authTag);

      const readStream = fs.createReadStream(filePath, {
        start: 28,
        end: fileStats.size - 17
      });
      const writeStream = fs.createWriteStream(outputPath);

      await pipeline(readStream, decipher, writeStream);
    }

    return { success: true, count: filePaths.length };
  } catch (err) {
    return { success: false, error: 'Incorrect password or corrupted file. (' + err.message + ')' };
  }
});
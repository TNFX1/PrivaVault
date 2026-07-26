const { app, BrowserWindow, ipcMain, dialog } = require('electron');
const path = require('path');
const fs = require('fs');
const crypto = require('crypto');
const { pipeline } = require('stream/promises');

let mainWindow;

function createWindow() {
  const iconPath = path.join(__dirname, 'logo.png');

  mainWindow = new BrowserWindow({
    width: 520,
    height: 850,
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

// Dosya Seçimi
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

// Şifreleme İşlemi (Dinamik Iteration ve Uzantı İle)
ipcMain.handle('encrypt-file', async (event, { filePath, password, customExt, iterations }) => {
  try {
    const ext = customExt ? customExt.replace(/^\./, '') : 'pvault';
    const defaultOutputName = `${filePath}.${ext}`;

    const saveResult = await dialog.showSaveDialog(mainWindow, {
      title: 'Save Encrypted File',
      defaultPath: defaultOutputName
    });

    if (saveResult.canceled || !saveResult.filePath) return { success: false, status: 'canceled' };

    const outputPath = saveResult.filePath;
    const salt = crypto.randomBytes(16);
    const iv = crypto.randomBytes(12);

    // Kullanıcının ayarlarda seçtiği iteration sayısı kullanılır (Varsayılan 100000)
    const iter = iterations ? parseInt(iterations, 10) : 100000;
    const key = crypto.pbkdf2Sync(password, salt, iter, 32, 'sha256');
    const cipher = crypto.createCipheriv('aes-256-gcm', key, iv);

    const readStream = fs.createReadStream(filePath);
    const writeStream = fs.createWriteStream(outputPath);

    writeStream.write(salt);
    writeStream.write(iv);

    await pipeline(readStream, cipher, writeStream);

    const authTag = cipher.getAuthTag();
    fs.appendFileSync(outputPath, authTag);

    return { success: true, outputPath };
  } catch (err) {
    return { success: false, error: err.message };
  }
});

// Şifre Çözme İşlemi
ipcMain.handle('decrypt-file', async (event, { filePath, password, iterations }) => {
  try {
    const fileStats = fs.statSync(filePath);
    if (fileStats.size < 16 + 12 + 16) {
      throw new Error('Invalid encrypted file structure.');
    }

    const defaultOutput = filePath.replace(/\.[^/.]+$/, "");

    const saveResult = await dialog.showSaveDialog(mainWindow, {
      title: 'Save Decrypted File',
      defaultPath: defaultOutput
    });

    if (saveResult.canceled || !saveResult.filePath) return { success: false, status: 'canceled' };

    const outputPath = saveResult.filePath;

    const fd = fs.openSync(filePath, 'r');
    const salt = Buffer.alloc(16);
    const iv = Buffer.alloc(12);
    const authTag = Buffer.alloc(16);

    fs.readSync(fd, salt, 0, 16, 0);
    fs.readSync(fd, iv, 0, 12, 16);
    fs.readSync(fd, authTag, 0, 16, fileStats.size - 16);
    fs.closeSync(fd);

    const iter = iterations ? parseInt(iterations, 10) : 100000;
    const key = crypto.pbkdf2Sync(password, salt, iter, 32, 'sha256');
    const decipher = crypto.createDecipheriv('aes-256-gcm', key, iv);
    decipher.setAuthTag(authTag);

    const readStream = fs.createReadStream(filePath, {
      start: 28,
      end: fileStats.size - 17
    });
    const writeStream = fs.createWriteStream(outputPath);

    await pipeline(readStream, decipher, writeStream);

    return { success: true, outputPath };
  } catch (err) {
    return { success: false, error: 'Incorrect password or key iteration mismatch. (' + err.message + ')' };
  }
});
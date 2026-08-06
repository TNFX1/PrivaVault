const { contextBridge, ipcRenderer } = require('electron');

contextBridge.exposeInMainWorld('electronAPI', {
  selectFiles: () => ipcRenderer.invoke('select-files'),
  selectFolders: () => ipcRenderer.invoke('select-folders'),
  selectVaultFile: () => ipcRenderer.invoke('select-vault-file'),
  encryptFiles: (data) => ipcRenderer.invoke('encrypt-files', data),
  inspectVault: (data) => ipcRenderer.invoke('inspect-vault', data),
  decryptVault: (data) => ipcRenderer.invoke('decrypt-vault', data),
  extractSingleFile: (data) => ipcRenderer.invoke('extract-single-file', data),
  getLaunchedFile: () => ipcRenderer.invoke('get-launched-file'),
  getAppLocale: () => ipcRenderer.invoke('get-app-locale'),
  setLanguage: (lang) => ipcRenderer.invoke('set-language', lang),
  onProgress: (callback) => ipcRenderer.on('progress-update', (event, value) => callback(value)),
  onUpdateAvailable: (callback) => ipcRenderer.on('update-available', (event, value) => callback(value))
});
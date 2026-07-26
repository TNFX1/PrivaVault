const { contextBridge, ipcRenderer } = require('electron');

contextBridge.exposeInMainWorld('electronAPI', {
  selectFiles: () => ipcRenderer.invoke('select-files'),
  selectVaultFile: () => ipcRenderer.invoke('select-vault-file'),
  encryptFile: (data) => ipcRenderer.invoke('encrypt-file', data),
  inspectVault: (data) => ipcRenderer.invoke('inspect-vault', data),
  extractSingleFile: (data) => ipcRenderer.invoke('extract-single-file', data),
  extractAllFiles: (data) => ipcRenderer.invoke('extract-all-files', data)
});
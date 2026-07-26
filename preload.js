const { contextBridge, ipcRenderer } = require('electron');

contextBridge.exposeInMainWorld('electronAPI', {
  selectFiles: () => ipcRenderer.invoke('select-files'),
  selectVaultFile: () => ipcRenderer.invoke('select-vault-file'),
  encryptFile: (data) => ipcRenderer.invoke('encrypt-file', data),
  decryptFile: (data) => ipcRenderer.invoke('decrypt-file', data)
});
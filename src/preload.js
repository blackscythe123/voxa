const { contextBridge, ipcRenderer } = require("electron");

contextBridge.exposeInMainWorld("voiceBridge", {
  onStartRecording: (cb) => ipcRenderer.on("start-recording", (_e, payload) => cb(payload)),
  onStopRecording: (cb) => ipcRenderer.on("stop-recording", () => cb()),
  onRecordingStatus: (cb) => ipcRenderer.on("recording-status", (_e, payload) => cb(payload)),
  finalizeRecording: (audioBytes, mimeType) => ipcRenderer.invoke("finalize-recording", audioBytes, mimeType),
  loginChatGPT: () => ipcRenderer.invoke("login-chatgpt"),
  getAuthStatus: () => ipcRenderer.invoke("get-auth-status"),
  getPreferences: () => ipcRenderer.invoke("get-preferences"),
  savePreferences: (payload) => ipcRenderer.invoke("save-preferences", payload),
  openSettings: () => ipcRenderer.invoke("open-settings"),
  openMicSettings: () => ipcRenderer.invoke("open-mic-settings"),
  getAutoLaunch: () => ipcRenderer.invoke("get-auto-launch"),
  setAutoLaunch: (enabled) => ipcRenderer.invoke("set-auto-launch", enabled)
});

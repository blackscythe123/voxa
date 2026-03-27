const { contextBridge, ipcRenderer } = require("electron");

contextBridge.exposeInMainWorld("voiceBridge", {
  onStartRecording: (callback) => {
    ipcRenderer.on("start-recording", callback);
  },
  onStopRecording: (callback) => {
    ipcRenderer.on("stop-recording", callback);
  },
  onRecordingStatus: (callback) => {
    ipcRenderer.on("recording-status", (_event, payload) => callback(payload));
  },
  finalizeRecording: (audioBytes, mimeType) => ipcRenderer.invoke("finalize-recording", audioBytes, mimeType),
  getPreferences: () => ipcRenderer.invoke("get-preferences"),
  savePreferences: (payload) => ipcRenderer.invoke("save-preferences", payload),
  loginChatGPT: () => ipcRenderer.invoke("login-chatgpt"),
  openChatGPTLoginPage: () => ipcRenderer.invoke("open-chatgpt-login-page"),
  saveChatGPTSessionText: (sessionText) => ipcRenderer.invoke("save-chatgpt-session-text", sessionText),
  openSettings: () => ipcRenderer.invoke("open-settings")
});

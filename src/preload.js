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
  savePreferences: (payload) => ipcRenderer.invoke("save-preferences", payload)
});

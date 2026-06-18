const { contextBridge, ipcRenderer } = require("electron");

contextBridge.exposeInMainWorld("voiceBridge", {
  onStartRecording: (cb) => ipcRenderer.on("start-recording", (_e, payload) => cb(payload)),
  onStopRecording: (cb) => ipcRenderer.on("stop-recording", () => cb()),
  onRecordingStatus: (cb) => ipcRenderer.on("recording-status", (_e, payload) => cb(payload)),
  finalizeRecording: (audioBytes, mimeType) => ipcRenderer.invoke("finalize-recording", audioBytes, mimeType),
  loginChatGPT: () => ipcRenderer.invoke("login-chatgpt"),
  getAuthStatus: () => ipcRenderer.invoke("get-auth-status"),
  getAccountInfo: () => ipcRenderer.invoke("get-account-info"),
  getPreferences: () => ipcRenderer.invoke("get-preferences"),
  savePreferences: (payload) => ipcRenderer.invoke("save-preferences", payload),
  openSettings: () => ipcRenderer.invoke("open-settings"),
  openMicSettings: () => ipcRenderer.invoke("open-mic-settings"),
  getAutoLaunch: () => ipcRenderer.invoke("get-auto-launch"),
  setAutoLaunch: (enabled) => ipcRenderer.invoke("set-auto-launch", enabled),
  signOut: () => ipcRenderer.invoke("sign-out"),
  getAppVersion: () => ipcRenderer.invoke("get-app-version"),
  openLogsFolder: () => ipcRenderer.invoke("open-logs-folder"),
  openDataFolder: () => ipcRenderer.invoke("open-data-folder"),
  historyList: (args) => ipcRenderer.invoke("history-list", args),
  historyGet: (id) => ipcRenderer.invoke("history-get", id),
  historyDelete: (id) => ipcRenderer.invoke("history-delete", id),
  historyRetry: (id) => ipcRenderer.invoke("history-retry", id),
  historyCopy: (id) => ipcRenderer.invoke("history-copy", id),
  historyReinsert: (id) => ipcRenderer.invoke("history-reinsert", id),
  historyRevealAudio: (id) => ipcRenderer.invoke("history-reveal-audio", id),
  historyAudioData: (id) => ipcRenderer.invoke("history-audio-data", id),
  historyStats: (args) => ipcRenderer.invoke("history-stats", args),
  historyGetRecentFailures: (args) => ipcRenderer.invoke("history-get-recent-failures", args),
  recordingStartMeta: (args) => ipcRenderer.invoke("recording-start-meta", args),
  recordingChunk: (args) => ipcRenderer.invoke("recording-chunk", args),
  onHistoryChanged: (cb) => {
    const listener = (_e, payload) => cb(payload);
    ipcRenderer.on("history-changed", listener);
    return () => ipcRenderer.removeListener("history-changed", listener);
  },
  onDictationComplete: (cb) => {
    const listener = (_e, payload) => cb(payload);
    ipcRenderer.on("dictation-complete", listener);
    return () => ipcRenderer.removeListener("dictation-complete", listener);
  },
  onNavigate: (cb) => {
    const listener = (_e, payload) => cb(payload);
    ipcRenderer.on("navigate", listener);
    return () => ipcRenderer.removeListener("navigate", listener);
  }
});

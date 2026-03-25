const path = require("node:path");
const { app, BrowserWindow, globalShortcut, ipcMain, Menu, dialog } = require("electron");

let mainWindow;
let overlayWindow;
let preferencesWindow;
let isRecording = false;

// Lazy load services to avoid startup issues
let configStore, chatgptAuth, textInserter;
function loadServices() {
  if (!configStore) configStore = require("./services/configStore");
  if (!chatgptAuth) chatgptAuth = require("./services/chatgptAuth");
  if (!textInserter) textInserter = require("./services/textInserter");
}

function createMainWindow() {
  mainWindow = new BrowserWindow({
    width: 800,
    height: 600,
    icon: path.join(__dirname, "..", "assets", "icon.png"),
    webPreferences: {
      preload: path.join(__dirname, "preload.js"),
      contextIsolation: true,
      nodeIntegration: false
    }
  });

  mainWindow.loadFile(path.join(__dirname, "renderer", "index.html"));

  mainWindow.on("closed", () => {
    mainWindow = null;
  });

  return mainWindow;
}

function createOverlayWindow() {
  overlayWindow = new BrowserWindow({
    width: 360,
    height: 180,
    frame: false,
    transparent: true,
    alwaysOnTop: true,
    skipTaskbar: true,
    resizable: false,
    focusable: false,
    webPreferences: {
      preload: path.join(__dirname, "preload.js"),
      contextIsolation: true,
      nodeIntegration: false
    }
  });

  overlayWindow.loadFile(path.join(__dirname, "renderer", "index.html"));
  overlayWindow.hide();
}

function createPreferencesWindow() {
  if (preferencesWindow && !preferencesWindow.isDestroyed()) {
    preferencesWindow.show();
    return;
  }

  preferencesWindow = new BrowserWindow({
    width: 480,
    height: 520,
    title: "budgetWhisper Preferences",
    webPreferences: {
      preload: path.join(__dirname, "preload.js"),
      contextIsolation: true,
      nodeIntegration: false
    }
  });

  preferencesWindow.loadFile(path.join(__dirname, "renderer", "preferences.html"));
}

function placeOverlay(position) {
  if (!overlayWindow) return;
  const display = require("electron").screen.getPrimaryDisplay();
  const bounds = display.workArea;
  const [width, height] = overlayWindow.getSize();
  const margin = 16;

  let x = bounds.x + bounds.width - width - margin;
  let y = bounds.y + bounds.height - height - margin;

  if (position === "top-right") {
    y = bounds.y + margin;
  }
  if (position === "top-left") {
    x = bounds.x + margin;
    y = bounds.y + margin;
  }
  if (position === "bottom-left") {
    x = bounds.x + margin;
  }

  overlayWindow.setPosition(x, y);
}

function notifyRenderer(status, detail = "") {
  if (mainWindow && !mainWindow.isDestroyed()) {
    mainWindow.webContents.send("recording-status", { status, detail });
  }
  if (overlayWindow && !overlayWindow.isDestroyed()) {
    overlayWindow.webContents.send("recording-status", { status, detail });
  }
}

function setRecordingState(nextState) {
  isRecording = nextState;
  notifyRenderer(isRecording ? "recording" : "idle");
}

function stopRecording() {
  if (!isRecording) return;
  if (overlayWindow && !overlayWindow.isDestroyed()) {
    overlayWindow.webContents.send("stop-recording");
  }
}

function startRecording() {
  if (isRecording) {
    stopRecording();
    return;
  }

  loadServices();
  const preferences = configStore.getPreferences();
  placeOverlay(preferences.overlayPosition);
  overlayWindow.showInactive();
  overlayWindow.webContents.send("start-recording", {
    maxRecordingSeconds: preferences.maxRecordingSeconds
  });
  setRecordingState(true);
}

function registerHotkeys() {
  loadServices();
  globalShortcut.unregisterAll();
  const preferences = configStore.getPreferences();

  const startRegistered = globalShortcut.register(preferences.startHotkey, () => {
    startRecording();
  });

  const stopRegistered = globalShortcut.register(preferences.stopHotkey, () => {
    stopRecording();
  });

  if (!startRegistered || !stopRegistered) {
    notifyRenderer("error", "Unable to register one or more shortcuts. Update preferences.");
  }
}

ipcMain.handle("finalize-recording", async (_event, audioBytes, mimeType) => {
  try {
    loadServices();
    notifyRenderer("working", "Transcribing audio...");

    if (!chatgptAuth.hasStorageState()) {
      throw new Error("Login required. Click 'Login to ChatGPT' in the app menu.");
    }

    const transcript = await chatgptAuth.transcribeAudio(Buffer.from(audioBytes), mimeType);
    if (!transcript) {
      throw new Error("Empty transcript returned.");
    }

    notifyRenderer("working", "Inserting transcript...");
    await textInserter.insertTranscript(transcript);

    notifyRenderer("success", "Transcript inserted.");
    setRecordingState(false);
    if (overlayWindow && !overlayWindow.isDestroyed()) {
      overlayWindow.hide();
    }

    return { ok: true, transcript };
  } catch (error) {
    setRecordingState(false);
    notifyRenderer("error", error.message || "Unknown error while transcribing.");
    return { ok: false, error: error.message || "Unknown error" };
  }
});

ipcMain.handle("get-preferences", () => {
  loadServices();
  return configStore.getPreferences();
});

ipcMain.handle("save-preferences", (_event, payload) => {
  loadServices();
  const next = configStore.savePreferences(payload || {});
  registerHotkeys();
  return next;
});

ipcMain.handle("login-chatgpt", async () => {
  loadServices();
  notifyRenderer("working", "Opening ChatGPT login in your browser...");
  const ok = await chatgptAuth.loginWithPlaywright();
  if (ok) {
    notifyRenderer("success", "ChatGPT login saved successfully!");
  } else {
    notifyRenderer("error", "ChatGPT login timed out or was cancelled.");
  }
  return ok;
});

ipcMain.handle("open-settings", () => {
  createPreferencesWindow();
});

app.whenReady().then(() => {
  createMainWindow();
  createOverlayWindow();
  registerHotkeys();

  loadServices();
  const preferences = configStore.getPreferences();
  if (preferences.autoLoginOnStart && !chatgptAuth.hasStorageState()) {
    chatgptAuth.loginWithPlaywright().catch(() => {
      notifyRenderer("error", "Auto-login failed.");
    });
  }
});

app.on("will-quit", () => {
  globalShortcut.unregisterAll();
});

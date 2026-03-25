const path = require("node:path");
const { app, BrowserWindow, globalShortcut, ipcMain, Menu, Tray, nativeImage, shell } = require("electron");
const { getPreferences, savePreferences } = require("./services/configStore");
const { hasStorageState, loginWithPlaywright, transcribeAudio } = require("./services/chatgptAuth");
const { insertTranscript } = require("./services/textInserter");

let overlayWindow;
let preferencesWindow;
let tray;
let isRecording = false;

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
  if (!overlayWindow || overlayWindow.isDestroyed()) return;
  overlayWindow.webContents.send("recording-status", { status, detail });
}

function setRecordingState(nextState) {
  isRecording = nextState;
  notifyRenderer(isRecording ? "recording" : "idle");
}

function stopRecording() {
  if (!isRecording) return;
  overlayWindow.webContents.send("stop-recording");
}

function startRecording() {
  if (isRecording) {
    stopRecording();
    return;
  }

  const preferences = getPreferences();
  placeOverlay(preferences.overlayPosition);
  overlayWindow.showInactive();
  overlayWindow.webContents.send("start-recording", {
    maxRecordingSeconds: preferences.maxRecordingSeconds
  });
  setRecordingState(true);
}

function registerHotkeys() {
  globalShortcut.unregisterAll();
  const preferences = getPreferences();

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

function createTray() {
  tray = new Tray(nativeImage.createEmpty());
  const menu = Menu.buildFromTemplate([
    {
      label: "Start / Stop Recording",
      click: () => {
        if (isRecording) {
          stopRecording();
        } else {
          startRecording();
        }
      }
    },
    {
      label: "Login to ChatGPT",
      click: async () => {
        notifyRenderer("working", "Waiting for ChatGPT login in browser...");
        const ok = await loginWithPlaywright();
        notifyRenderer(ok ? "success" : "error", ok ? "ChatGPT login saved." : "ChatGPT login timed out.");
      }
    },
    {
      label: "Preferences",
      click: () => createPreferencesWindow()
    },
    {
      label: "Open App Data Folder",
      click: () => {
        shell.openPath(app.getPath("userData"));
      }
    },
    { type: "separator" },
    {
      label: "Quit",
      click: () => {
        app.quit();
      }
    }
  ]);

  tray.setToolTip("budgetWhisper");
  tray.setContextMenu(menu);
}

ipcMain.handle("finalize-recording", async (_event, audioBytes, mimeType) => {
  try {
    notifyRenderer("working", "Transcribing audio...");

    if (!hasStorageState()) {
      throw new Error("Login required. Use tray menu: Login to ChatGPT.");
    }

    const transcript = await transcribeAudio(Buffer.from(audioBytes), mimeType);
    if (!transcript) {
      throw new Error("Empty transcript returned.");
    }

    notifyRenderer("working", "Inserting transcript...");
    await insertTranscript(transcript);

    notifyRenderer("success", "Transcript inserted.");
    setRecordingState(false);
    overlayWindow.hide();

    return { ok: true, transcript };
  } catch (error) {
    setRecordingState(false);
    notifyRenderer("error", error.message || "Unknown error while transcribing.");
    return { ok: false, error: error.message || "Unknown error" };
  }
});

ipcMain.handle("get-preferences", () => getPreferences());
ipcMain.handle("save-preferences", (_event, payload) => {
  const next = savePreferences(payload || {});
  registerHotkeys();
  return next;
});

app.whenReady().then(() => {
  createOverlayWindow();
  createTray();
  registerHotkeys();

  const preferences = getPreferences();
  if (preferences.autoLoginOnStart && !hasStorageState()) {
    loginWithPlaywright().catch(() => {
      notifyRenderer("error", "Auto-login failed.");
    });
  }
});

app.on("will-quit", () => {
  globalShortcut.unregisterAll();
});

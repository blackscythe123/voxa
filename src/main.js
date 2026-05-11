const path = require("node:path");
const fs = require("node:fs");
const { app, BrowserWindow, globalShortcut, ipcMain, shell, Tray, Menu, nativeImage, Notification, systemPreferences } = require("electron");

let mainWindow;
let overlayWindow;
let preferencesWindow;
let tray;
let isRecording = false;
let isQuitting = false;

// Per-OS icon. Windows reads .ico for taskbar/start-menu/explorer at every
// size — handing it a .png makes the shell fall back to the Electron globe
// even after install. macOS and Linux both happily take .png at runtime.
const ICON_PATH = (() => {
  const dir = path.join(__dirname, "..", "assets");
  if (process.platform === "win32") {
    const ico = path.join(dir, "icon.ico");
    if (fs.existsSync(ico)) return ico;
  }
  return path.join(dir, "icon.png");
})();
const TRAY_ICON_PATH = path.join(__dirname, "..", "assets", "tray.png");

let configStore, chatgptAuth, textInserter;
function loadServices() {
  if (!configStore) configStore = require("./services/configStore");
  if (!chatgptAuth) chatgptAuth = require("./services/chatgptAuth");
  if (!textInserter) textInserter = require("./services/textInserter");
}

// Tells Windows which app is firing notifications — without it, toasts read
// as "Electron" instead of "Voxa." Must match the build.appId in package.json.
if (process.platform === "win32") app.setAppUserModelId("com.Voxa.app");

// ── Single-instance lock — second launch focuses the running instance ──────
const gotLock = app.requestSingleInstanceLock();
if (!gotLock) {
  app.quit();
} else {
  app.on("second-instance", () => {
    showMainWindow();
  });
}

function createMainWindow() {
  mainWindow = new BrowserWindow({
    width: 1100,
    height: 760,
    minWidth: 380,
    minHeight: 520,
    icon: ICON_PATH,
    backgroundColor: "#faf6f0",
    titleBarStyle: process.platform === "darwin" ? "hiddenInset" : "hidden",
    titleBarOverlay: process.platform === "win32" ? {
      color:        "#faf6f0",  // cream — matches the page background
      symbolColor:  "#2d2418",  // warm brown ink for min/max/close glyphs
      height:       40
    } : undefined,
    show: !app.commandLine.hasSwitch("hidden"),
    webPreferences: {
      preload: path.join(__dirname, "preload.js"),
      contextIsolation: true,
      nodeIntegration: false
    }
  });

  mainWindow.loadFile(path.join(__dirname, "renderer", "index.html"));

  mainWindow.on("close", (e) => {
    if (!isQuitting) {
      e.preventDefault();
      mainWindow.hide();
    }
  });
}

function showMainWindow() {
  if (!mainWindow || mainWindow.isDestroyed()) {
    createMainWindow();
    return;
  }
  if (mainWindow.isMinimized()) mainWindow.restore();
  mainWindow.show();
  mainWindow.focus();
}

function createOverlayWindow() {
  overlayWindow = new BrowserWindow({
    width: 240,
    height: 84,
    frame: false,
    transparent: true,
    alwaysOnTop: true,
    skipTaskbar: true,
    resizable: false,
    focusable: false,
    hasShadow: false,
    webPreferences: {
      preload: path.join(__dirname, "preload.js"),
      contextIsolation: true,
      nodeIntegration: false
    }
  });

  overlayWindow.loadFile(path.join(__dirname, "renderer", "overlay.html"));
  overlayWindow.hide();
}

function createPreferencesWindow() {
  if (preferencesWindow && !preferencesWindow.isDestroyed()) {
    preferencesWindow.show();
    preferencesWindow.focus();
    return;
  }

  preferencesWindow = new BrowserWindow({
    width: 580,
    height: 720,
    minWidth: 380,
    minHeight: 480,
    title: "Voxa Preferences",
    icon: ICON_PATH,
    backgroundColor: "#faf6f0",
    titleBarStyle: process.platform === "darwin" ? "hiddenInset" : "hidden",
    titleBarOverlay: process.platform === "win32" ? {
      color:       "#faf6f0",
      symbolColor: "#2d2418",
      height:      40
    } : undefined,
    webPreferences: {
      preload: path.join(__dirname, "preload.js"),
      contextIsolation: true,
      nodeIntegration: false
    }
  });

  preferencesWindow.loadFile(path.join(__dirname, "renderer", "preferences.html"));
}

// ── System tray — keeps app alive in background ────────────────────────────
function createTray() {
  const trayImage = nativeImage.createFromPath(TRAY_ICON_PATH);
  tray = new Tray(trayImage);
  tray.setToolTip("Voxa, voice to text");
  rebuildTrayMenu();

  // Single click: pop the menu right at the cursor (the small popup the user
  // expected — Windows defaults to "nothing happens" on left-click otherwise).
  tray.on("click", () => tray.popUpContextMenu());
  // Double-click jumps straight to opening the window.
  tray.on("double-click", () => showMainWindow());
}

function rebuildTrayMenu() {
  if (!tray) return;
  loadServices();
  const prefs = configStore.getPreferences();
  const startKey = prefs.startHotkey?.replace(/CommandOrControl/g, "Ctrl") || "F9";

  const menu = Menu.buildFromTemplate([
    { label: `Start recording  (${startKey})`, click: () => startRecording() },
    { type: "separator" },
    { label: "Show window",   click: () => showMainWindow() },
    { label: "Preferences…",  click: () => createPreferencesWindow() },
    { type: "separator" },
    {
      label: "Launch on system startup",
      type: "checkbox",
      checked: app.getLoginItemSettings().openAtLogin,
      click: (item) => setAutoLaunch(item.checked)
    },
    { type: "separator" },
    { label: "Quit Voxa", click: () => { isQuitting = true; app.quit(); } }
  ]);
  tray.setContextMenu(menu);
}

function setAutoLaunch(enabled) {
  app.setLoginItemSettings({
    openAtLogin: enabled,
    openAsHidden: true,            // start minimized to tray on macOS
    args: enabled ? ["--hidden"] : [] // pass flag on Windows so window stays hidden
  });
  rebuildTrayMenu();
}

// ── Recording flow ─────────────────────────────────────────────────────────
function placeOverlay(position) {
  if (!overlayWindow) return;
  const { screen } = require("electron");
  const bounds = screen.getPrimaryDisplay().workArea;
  const [width, height] = overlayWindow.getSize();
  const margin = 16;

  let x = bounds.x + bounds.width - width - margin;
  let y = bounds.y + bounds.height - height - margin;

  if (position === "top-right") y = bounds.y + margin;
  if (position === "top-left") { x = bounds.x + margin; y = bounds.y + margin; }
  if (position === "bottom-left") x = bounds.x + margin;

  overlayWindow.setPosition(x, y);
}

function notifyMain(status, detail = "") {
  if (mainWindow && !mainWindow.isDestroyed()) {
    mainWindow.webContents.send("recording-status", { status, detail });
  }
}

// ── OS notifications for things the user must see even when the window is hidden
function notifyOS(title, body, { onClick } = {}) {
  if (!Notification.isSupported()) return null;
  const n = new Notification({
    title,
    body,
    icon: ICON_PATH,
    silent: false
  });
  n.on("click", () => {
    showMainWindow();
    if (onClick) onClick();
  });
  n.show();
  return n;
}

// Choose a notification message based on what kind of error happened.
// We classify the message text so the toast feels intentional, not generic.
function notifyExternalError(message) {
  const m = (message || "").toLowerCase();
  if (m.includes("microphone") || m.includes("silence") || m.includes("silent")) {
    notifyOS("Voxa couldn't hear you", "The microphone captured silence. Click here, then open Windows mic settings.", {
      onClick: () => shell.openExternal("ms-settings:privacy-microphone")
    });
  } else if (m.includes("session") || m.includes("login") || m.includes("403") || m.includes("401") || m.includes("not logged in")) {
    notifyOS("Voxa needs you to sign in again", "Your ChatGPT session expired. Click to open Voxa and sign in.");
  } else if (m.includes("transcription failed")) {
    notifyOS("Voxa couldn't transcribe that", "Something went wrong sending your audio to ChatGPT. Click to open Voxa.");
  } else {
    notifyOS("Voxa hit an error", message || "Click to open Voxa for details.");
  }
}

function setRecordingState(next) {
  isRecording = next;
  notifyMain(isRecording ? "recording" : "idle");
}

function stopRecording() {
  if (!isRecording) return;
  if (overlayWindow && !overlayWindow.isDestroyed()) {
    overlayWindow.webContents.send("stop-recording");
  }
}

async function ensureMicAccess() {
  // macOS gates microphone access through TCC. The native prompt only fires
  // when *something* in the app explicitly asks — calling this at the
  // moment the user presses record makes it feel intentional, and it's
  // the gesture macOS expects. On Win/Linux this is a no-op.
  if (process.platform !== "darwin") return true;
  try {
    let status = systemPreferences.getMediaAccessStatus("microphone");
    if (status === "not-determined") {
      const ok = await systemPreferences.askForMediaAccess("microphone");
      status = ok ? "granted" : "denied";
    }
    if (status !== "granted") {
      // Already denied previously — macOS won't show the prompt a second
      // time, the user has to flip the switch themselves. Hand them the
      // exact pane and a notification so the next press succeeds.
      notifyOS(
        "Voxa needs microphone access",
        "Open System Settings → Privacy & Security → Microphone and enable Voxa, then try again."
      );
      shell.openExternal("x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone");
      notifyMain("error", "Microphone access denied — enable it in System Settings.");
      return false;
    }
    return true;
  } catch (err) {
    notifyMain("error", `Microphone check failed: ${err.message}`);
    return false;
  }
}

async function startRecording() {
  if (isRecording) {
    stopRecording();
    return;
  }

  if (!(await ensureMicAccess())) return;

  loadServices();
  const prefs = configStore.getPreferences();
  placeOverlay(prefs.overlayPosition);
  overlayWindow.showInactive();
  overlayWindow.webContents.send("start-recording", {
    maxRecordingSeconds: prefs.maxRecordingSeconds,
    inputDeviceId: prefs.inputDeviceId
  });
  setRecordingState(true);
}

function registerHotkeys() {
  loadServices();
  globalShortcut.unregisterAll();
  const prefs = configStore.getPreferences();

  const startKey = prefs.startHotkey;
  const stopKey  = prefs.stopHotkey;

  let startOk = true, stopOk = true;
  if (startKey) {
    startOk = globalShortcut.register(startKey, startRecording);
  }
  // Only register the stop hotkey if it's different from start.
  // If they match, startRecording() already toggles (re-press stops).
  if (stopKey && stopKey !== startKey) {
    stopOk = globalShortcut.register(stopKey, stopRecording);
  }

  if (!startOk || !stopOk) {
    notifyMain("error", "Could not register shortcuts. Check Preferences.");
  }
}

// ── IPC handlers ───────────────────────────────────────────────────────────
ipcMain.handle("finalize-recording", async (_event, audioBytes, mimeType) => {
  try {
    loadServices();
    notifyMain("working", "Transcribing...");

    const transcript = await chatgptAuth.transcribeAudio(Buffer.from(audioBytes), mimeType);
    if (!transcript) throw new Error("Empty transcript returned.");

    notifyMain("working", "Pasting transcript...");
    await textInserter.insertTranscript(transcript);

    setRecordingState(false);
    if (overlayWindow && !overlayWindow.isDestroyed()) overlayWindow.hide();
    notifyMain("success", transcript.length > 80 ? transcript.slice(0, 80) + "…" : transcript);

    return { ok: true, transcript };
  } catch (error) {
    setRecordingState(false);
    if (overlayWindow && !overlayWindow.isDestroyed()) overlayWindow.hide();
    const msg = error.message || "Unknown error.";
    notifyMain("error", msg);
    // Surface to the OS too — the main window may be hidden in the tray
    if (!mainWindow || !mainWindow.isVisible()) notifyExternalError(msg);
    return { ok: false, error: msg };
  }
});

ipcMain.handle("login-chatgpt", async () => {
  loadServices();
  notifyMain("working", "Opening ChatGPT login...");
  try {
    const ok = await chatgptAuth.loginWithBrowserWindow();
    if (ok) {
      notifyMain("success", "Logged in. Cookies saved.");
      notifyOS("Voxa is signed in", "Press your hotkey in any text field and start speaking.");
    } else {
      notifyMain("error", "Login window closed without completing login.");
    }
    return ok;
  } catch (err) {
    notifyMain("error", err.message || "Login failed.");
    notifyExternalError(err.message || "Login failed");
    return false;
  }
});

ipcMain.handle("get-auth-status", async () => {
  loadServices();
  return chatgptAuth.validateSession();
});

ipcMain.handle("get-preferences", () => {
  loadServices();
  return configStore.getPreferences();
});

ipcMain.handle("save-preferences", (_event, payload) => {
  loadServices();
  const next = configStore.savePreferences(payload || {});
  registerHotkeys();
  rebuildTrayMenu();
  return next;
});

ipcMain.handle("open-settings", () => createPreferencesWindow());
ipcMain.handle("open-mic-settings", () => {
  // Each OS has its own deep-link to the microphone privacy pane. Falling
  // back to a docs page would feel evasive, so we route to the right pane.
  if (process.platform === "win32") {
    return shell.openExternal("ms-settings:privacy-microphone");
  }
  if (process.platform === "darwin") {
    return shell.openExternal("x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone");
  }
  // GNOME / KDE both honour gnome-control-center; if it isn't installed
  // we fall through to a plain xdg-open of the sound pane.
  const { exec } = require("node:child_process");
  exec("gnome-control-center microphone || gnome-control-center sound || xdg-open settings://privacy/microphone || true");
  return true;
});

ipcMain.handle("get-auto-launch", () => app.getLoginItemSettings().openAtLogin);
ipcMain.handle("set-auto-launch", (_e, enabled) => { setAutoLaunch(!!enabled); return true; });

// ── OS-level permission bootstrap ──────────────────────────────────────────
// Electron's session permission handler only governs *renderer*-initiated
// requests. The OS itself (macOS TCC, Windows privacy settings) is a
// separate layer — we have to nudge it explicitly the first time so the
// user actually sees the native consent prompt, instead of silent denial.
function bootstrapOsPermissions() {
  // Mic permission is deferred to ensureMicAccess() at record-time — asking
  // here, with no user gesture, has macOS silently swallow the prompt and
  // wastes the one-shot ask (subsequent calls just return the cached
  // status without re-prompting). Accessibility we still surface up-front
  // because it has to be granted from System Settings before the first
  // paste, not in the middle of a recording flow.
  if (process.platform !== "darwin") return;
  try {
    const trusted = systemPreferences.isTrustedAccessibilityClient(false);
    if (!trusted) {
      notifyOS(
        "Voxa needs Accessibility access",
        "Open System Settings → Privacy & Security → Accessibility and enable Voxa so transcripts can paste into other apps."
      );
    }
  } catch { /* non-mac or API unavailable */ }
}

// ── App lifecycle ──────────────────────────────────────────────────────────
app.whenReady().then(async () => {
  const { session } = require("electron");
  session.defaultSession.setPermissionRequestHandler((_wc, permission, cb) => {
    cb(permission === "media" || permission === "audioCapture" || permission === "clipboard-read" || permission === "clipboard-sanitized-write");
  });
  session.defaultSession.setPermissionCheckHandler((_wc, permission) => {
    return permission === "media" || permission === "audioCapture" || permission === "clipboard-read" || permission === "clipboard-sanitized-write";
  });

  bootstrapOsPermissions();

  // Kill the default File / Edit / View / Window menu — slop tell otherwise
  Menu.setApplicationMenu(null);

  createMainWindow();
  createOverlayWindow();
  createTray();
  registerHotkeys();

  // Background session check — if cookies are stale, ping the user via the OS.
  // Small delay so the window is up first and the notification doesn't fight it.
  setTimeout(async () => {
    try {
      loadServices();
      if (chatgptAuth.hasCookies()) {
        const ok = await chatgptAuth.validateSession();
        if (!ok) notifyExternalError("Session expired");
      }
    } catch { /* ignore — surface only when user actually tries to record */ }
  }, 4000);
});

// On macOS clicking dock icon should re-open the window
app.on("activate", () => {
  if (BrowserWindow.getAllWindows().length === 0) createMainWindow();
  else showMainWindow();
});

// Don't quit when all windows are closed — stay alive in tray
app.on("window-all-closed", (e) => {
  // Default Electron behaviour quits on non-macOS — we want background mode
  e.preventDefault?.();
});

app.on("before-quit", () => { isQuitting = true; });
app.on("will-quit", () => { globalShortcut.unregisterAll(); });

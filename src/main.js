const path = require("node:path");
const fs = require("node:fs");
const crypto = require("node:crypto");
const { app, BrowserWindow, globalShortcut, ipcMain, shell, Tray, Menu, nativeImage, Notification, systemPreferences, clipboard } = require("electron");

// historyStore pulls in better-sqlite3 (a native module). If that binary isn't
// built for the current Electron ABI (fresh clone before `npm install` runs the
// electron-rebuild postinstall, or a platform without it), requiring it throws.
// We must NOT let that take down the core record→transcribe→paste flow, so the
// four new services load behind a guard. When historyEnabled is false the
// dictation pipeline still runs; only the on-disk history/retention layer is
// skipped. The store is pure JavaScript (no native module), so this normally
// loads cleanly; the guard just keeps a surprise require error from breaking
// the core dictation flow.
let historyStore, audioStore, retentionWorker, sourceApp;
let historyEnabled = false;
try {
  historyStore = require("./services/historyStore");
  audioStore = require("./services/audioStore");
  retentionWorker = require("./services/retentionWorker");
  sourceApp = require("./services/sourceApp");
  historyEnabled = true;
} catch (err) {
  // Logged again, with the proper logger, once the app is ready.
  process.stderr.write(
    `[voxa] history services unavailable (native module not built?): ${err && err.message}\n`
  );
  historyStore = audioStore = retentionWorker = null;
  sourceApp = { getForegroundAppLabel: async () => ({ exe: null, label: null }) };
}

let mainWindow;
let overlayWindow;
let preferencesWindow;
let tray;
let isRecording = false;
let isQuitting = false;

// The id of the dictation currently being recorded. Set in startRecording(),
// consumed by the chunk/stop handlers and cleared after finalize. Lets the
// old (renderer-buffered) finalize path recover the id even though the legacy
// overlay doesn't echo it back.
let activeDictationId = null;
let purgedOnQuit = false; // guard so before-quit teardown runs only once

// In a packaged build electron-builder unpacks assets to app.asar.unpacked/
// so Win32 Shell APIs (nativeImage, Tray) get a real filesystem path.
// In dev mode __dirname/../assets is the source tree.
const assetsDir = app.isPackaged
  ? path.join(process.resourcesPath, "app.asar.unpacked", "assets")
  : path.join(__dirname, "..", "assets");

const ICON_PATH = (() => {
  if (process.platform === "win32") {
    const ico = path.join(assetsDir, "icon.ico");
    if (fs.existsSync(ico)) return ico;
  }
  return path.join(assetsDir, "icon.png");
})();
const TRAY_ICON_PATH = path.join(assetsDir, "tray.png");

let configStore, chatgptAuth, textInserter;
function loadServices() {
  if (!configStore) configStore = require("./services/configStore");
  if (!chatgptAuth) chatgptAuth = require("./services/chatgptAuth");
  if (!textInserter) textInserter = require("./services/textInserter");
}

// Classify a transcription error message into one of the history error_kind
// buckets. Matches the exact strings thrown in chatgptAuth.js:
//   "Microphone captured silence"            -> silence
//   "Session expired" / "Not logged in" / "No access token" -> auth
//   "Transcription failed (5..)" / "(4..)"   -> server
//   fetch errors / aborts / timeouts         -> network/timeout
function classifyError(message) {
  const m = (message || "").toLowerCase();
  // An empty transcript means the audio had no speech (muted/silent mic) — a
  // "no speech" outcome, not a real failure, and never worth retrying.
  if (m.includes("empty transcript") || m.includes("no speech")) return "no_speech";
  if (m.includes("microphone captured silence") || m.includes("silence")) return "silence";
  if (
    m.includes("session expired") ||
    m.includes("not logged in") ||
    m.includes("no access token") ||
    m.includes("401") ||
    m.includes("403")
  ) return "auth";
  if (m.includes("transcription failed (5") || m.includes("transcription failed (4")) return "server";
  if (m.includes("transcription failed")) return "server";
  if (m.includes("timeout") || m.includes("timed out") || m.includes("aborted") || m.includes("abort")) return "timeout";
  if (
    m.includes("fetch") ||
    m.includes("network") ||
    m.includes("econn") ||
    m.includes("enotfound") ||
    m.includes("etimedout") ||
    m.includes("socket") ||
    m.includes("dns") ||
    m.includes("getaddrinfo")
  ) return "network";
  // Default unknown errors to network — most non-classified failures are
  // transient transport issues and the user can retry.
  return "network";
}

// ── File logging — written to %AppData%\Voxa\voxa.log ─────────────────────
let _logPath;
function log(...args) {
  const line = `[${new Date().toISOString()}] ${args.join(" ")}\n`;
  process.stdout.write(line);
  try {
    if (!_logPath) _logPath = path.join(app.getPath("userData"), "voxa.log");
    // Rotate: keep last ~200 KB
    try {
      if (fs.statSync(_logPath).size > 200_000) fs.writeFileSync(_logPath, line);
      else fs.appendFileSync(_logPath, line);
    } catch { fs.appendFileSync(_logPath, line); }
  } catch { /* never let logging kill the app */ }
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
    backgroundColor: "#00000000",
    alwaysOnTop: true,
    skipTaskbar: true,
    resizable: false,
    focusable: false,
    hasShadow: false,
    show: false,
    webPreferences: {
      preload: path.join(__dirname, "preload.js"),
      contextIsolation: true,
      nodeIntegration: false
    }
  });

  overlayWindow.loadFile(path.join(__dirname, "renderer", "overlay.html"));

  overlayWindow.on("closed", () => {
    log("overlay window closed unexpectedly");
    overlayWindow = null;
    if (!isQuitting) {
      log("recreating overlay window");
      createOverlayWindow();
    }
  });

  log("overlay window created");
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
    { label: "Settings…",     click: () => createPreferencesWindow() },
    { type: "separator" },
    {
      label: "Launch on system startup",
      type: "checkbox",
      checked: app.getLoginItemSettings({ args: ["--hidden"] }).openAtLogin,
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
  if (!overlayWindow || overlayWindow.isDestroyed()) return;
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
  } else {
    // Overlay gone — reset state directly so the hotkey isn't stuck
    log("stopRecording: overlay gone, resetting state directly");
    setRecordingState(false);
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

  // Guard: recreate overlay if it was destroyed since startup
  if (!overlayWindow || overlayWindow.isDestroyed()) {
    log("startRecording: overlay was destroyed, recreating");
    createOverlayWindow();
    await new Promise((r) => setTimeout(r, 300));
  }

  loadServices();
  const prefs = configStore.getPreferences();
  log(`startRecording: position=${prefs.overlayPosition}`);

  // Create the pending history row up-front. The id ties together the audio
  // file on disk, the streamed chunks, and the final transcript. We generate
  // it here (native UUIDv4 — zero-dep) and hand it to the overlay so the
  // streaming chunk handlers can address the right recording.
  const id = crypto.randomUUID();
  const startedAt = Date.now();

  // Best-effort: which app is in the foreground right now. Never let this
  // break a dictation — sourceApp already swallows its own errors, but we
  // guard anyway.
  const { exe, label } = await sourceApp
    .getForegroundAppLabel()
    .catch(() => ({ exe: null, label: null }));

  if (historyEnabled) {
    try {
      historyStore.insertPending({
        id,
        createdAt: startedAt,
        sourceApp: exe,
        sourceAppLabel: label
      });
    } catch (err) {
      // If the history row can't be written we still let the recording proceed
      // (dictation > bookkeeping), but log it loudly.
      log(`startRecording: insertPending failed: ${err.message}`);
    }
  }

  activeDictationId = id;

  placeOverlay(prefs.overlayPosition);
  overlayWindow.show();
  overlayWindow.setAlwaysOnTop(true, "screen-saver");
  overlayWindow.moveTop();
  overlayWindow.webContents.send("start-recording", {
    id,
    startedAt,
    maxRecordingSeconds: prefs.maxRecordingSeconds,
    inputDeviceId: prefs.inputDeviceId
  });
  setRecordingState(true);
  log(`startRecording: done id=${id} app=${exe || "?"}`);
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

  log(`registerHotkeys: start=${startKey}(ok=${startOk}) stop=${stopKey}(ok=${stopOk})`);
  if (!startOk || !stopOk) {
    notifyMain("error", "Could not register shortcuts. Check Preferences.");
  }
}

// ── IPC handlers ───────────────────────────────────────────────────────────

// Tell the main process to start a fresh on-disk audio file for this id.
// Fired by the (new, streaming) overlay right after it starts the recorder.
ipcMain.handle("recording-start-meta", async (_e, { id, startedAt } = {}) => {
  if (!historyEnabled) return { ok: false, error: "history_disabled" };
  const recId = id || activeDictationId;
  if (!recId) return { ok: false, error: "no_active_id" };
  try {
    audioStore.open(recId);
    activeDictationId = recId;
    log(`recording-start-meta: opened audio stream for ${recId} (startedAt=${startedAt})`);
    return { ok: true };
  } catch (err) {
    log(`recording-start-meta error: ${err.message}`);
    return { ok: false, error: err.message };
  }
});

// Append a streamed audio chunk to the on-disk file. `chunk` arrives as a
// transferable (ArrayBuffer / typed-array-backed array) over IPC.
ipcMain.handle("recording-chunk", async (_e, { id, chunk } = {}) => {
  if (!historyEnabled) return { ok: false, error: "history_disabled" };
  const recId = id || activeDictationId;
  if (!recId) return { ok: false, error: "no_active_id" };
  try {
    audioStore.appendChunk(recId, Buffer.from(chunk));
    return { ok: true };
  } catch (err) {
    log(`recording-chunk error: ${err.message}`);
    return { ok: false, error: err.message };
  }
});

// finalize-recording supports BOTH signatures during the overlay migration:
//
//   NEW (streaming):  args[0] = { id, durationMs, mimeType }
//       — audio already on disk via recording-chunk; we just close + transcribe.
//
//   OLD (renderer-buffered):  args[0] = audioBuffer (Buffer / {data:[...]}) ,
//                             args[1] = mimeType
//       — renderer still holds the whole blob; we write it to disk through
//         audioStore so the rest of the pipeline (history, retry, reveal) is
//         identical. The id is recovered from activeDictationId.
//
// This bridge is removed once overlay.html is rewritten to stream.
ipcMain.handle("finalize-recording", async (_event, ...args) => {
  loadServices();

  const first = args[0];
  const isLegacy =
    Buffer.isBuffer(first) ||
    (first && Array.isArray(first.data)) ||
    (Array.isArray(first)); // raw Array.from(Uint8Array) from the old overlay

  // ── History disabled (native module unavailable) ─────────────────────────
  // Preserve the original behaviour exactly: transcribe the renderer-supplied
  // buffer in-memory and paste it, with no SQLite/audio-store involvement. This
  // only ever runs the legacy (buffered) overlay path, which is what ships today.
  if (!historyEnabled) {
    try {
      let legacyBuf;
      if (Buffer.isBuffer(first)) legacyBuf = first;
      else if (first && Array.isArray(first.data)) legacyBuf = Buffer.from(first.data);
      else if (Array.isArray(first)) legacyBuf = Buffer.from(first);
      else throw new Error("History disabled and no audio buffer supplied by renderer.");
      const mt = args[1] || "audio/webm";
      log(`finalize-recording(no-history): ${legacyBuf.length} bytes, ${mt}`);
      notifyMain("working", "Transcribing...");
      const transcript = await chatgptAuth.transcribeAudio(legacyBuf, mt);
      if (!transcript) throw new Error("Empty transcript returned.");
      notifyMain("working", "Pasting transcript...");
      const insert = await textInserter.insertTranscript(transcript);
      setRecordingState(false);
      if (overlayWindow && !overlayWindow.isDestroyed()) overlayWindow.hide();
      const preview = transcript.length > 80 ? transcript.slice(0, 80) + "…" : transcript;
      if (insert.pasted) {
        notifyMain("success", preview);
      } else if (insert.clipboard) {
        notifyMain("success", "Copied to clipboard — press Ctrl+V to paste it.");
        notifyOS("Voxa couldn't auto-paste", "Your transcript is on the clipboard — press Ctrl+V where you want it.");
      } else {
        notifyMain("error", "Couldn't paste or copy — open History to copy it manually.");
        notifyOS("Voxa couldn't deliver the transcript", "Open Voxa → History to copy it.");
      }
      return { ok: true, transcript };
    } catch (error) {
      setRecordingState(false);
      if (overlayWindow && !overlayWindow.isDestroyed()) overlayWindow.hide();
      const msg = error.message || "Unknown error.";
      log(`finalize-recording(no-history) error: ${msg}`);
      notifyMain("error", msg);
      if (!mainWindow || !mainWindow.isVisible()) notifyExternalError(msg);
      return { ok: false, error: msg };
    }
  }

  let id, durationMs, mimeType;

  if (isLegacy) {
    // Old path: write the buffer the renderer just handed us to disk.
    id = activeDictationId || crypto.randomUUID();
    mimeType = args[1] || "audio/webm";
    let legacyBuf;
    if (Buffer.isBuffer(first)) legacyBuf = first;
    else if (first && Array.isArray(first.data)) legacyBuf = Buffer.from(first.data);
    else legacyBuf = Buffer.from(first);

    // The legacy overlay already injected the WebM duration into the buffer
    // and computed it from wall-clock; we don't get a separate durationMs, so
    // leave it 0 (injectDuration below is a no-op for the buffered path).
    durationMs = 0;

    // If no pending row exists for this id (e.g. recording started before this
    // build wrote one), create one now so finalize has a row to update.
    try {
      if (!historyStore.get(id)) {
        historyStore.insertPending({ id, createdAt: Date.now(), sourceApp: null, sourceAppLabel: null });
      }
    } catch (err) {
      log(`finalize-recording(legacy): insertPending recover failed: ${err.message}`);
    }

    // Stream the whole buffer to disk via audioStore so readBuffer/retry work.
    try {
      audioStore.open(id);
      audioStore.appendChunk(id, legacyBuf);
    } catch (err) {
      log(`finalize-recording(legacy): audioStore write failed: ${err.message}`);
    }
    log(`finalize-recording(legacy): ${legacyBuf.length} bytes, ${mimeType}, id=${id}`);
  } else {
    ({ id, durationMs, mimeType } = first || {});
    id = id || activeDictationId;
    mimeType = mimeType || "audio/webm";
    log(`finalize-recording(stream): id=${id} durationMs=${durationMs} ${mimeType}`);
  }

  if (!id) {
    const msg = "finalize-recording: no dictation id";
    log(msg);
    setRecordingState(false);
    if (overlayWindow && !overlayWindow.isDestroyed()) overlayWindow.hide();
    notifyMain("error", msg);
    return { ok: false, error: msg };
  }

  notifyMain("working", "Transcribing...");

  // Close the on-disk stream, patch the WebM duration, then read it back.
  let buf;
  let audioBytes = 0;
  try {
    await audioStore.close(id);
    if (Number.isFinite(durationMs) && durationMs > 0) {
      await audioStore.injectDuration(id, durationMs);
    }
    buf = audioStore.readBuffer(id);
    audioBytes = buf.length;
  } catch (err) {
    // Couldn't assemble the audio at all — record a failed row and bail.
    const msg = `Could not read recorded audio: ${err.message}`;
    log(`finalize-recording: ${msg}`);
    try {
      historyStore.finalize({
        id, status: "failed", transcript: null,
        errorKind: "timeout", errorMessage: msg,
        durationMs: durationMs || 0, wordCount: 0,
        audioFilename: null, audioBytes: null
      });
      mainWindow?.webContents.send("history-changed", { id, kind: "update" });
    } catch { /* best effort */ }
    activeDictationId = null;
    setRecordingState(false);
    if (overlayWindow && !overlayWindow.isDestroyed()) overlayWindow.hide();
    notifyMain("error", msg);
    if (!mainWindow || !mainWindow.isVisible()) notifyExternalError(msg);
    return { ok: false, error: msg };
  }

  const audioFilename = `${id}.webm`;

  try {
    const transcript = await chatgptAuth.transcribeAudio(buf, mimeType);

    // Empty transcript = ChatGPT processed the audio but heard no speech
    // (almost always a muted/silent mic). This is NOT an error, and Retry is
    // pointless — re-sending silence yields the same empty result.
    if (!transcript || !transcript.trim()) {
      historyStore.finalize({
        id, status: "empty", transcript: null,
        errorKind: "no_speech",
        errorMessage: "No speech detected — check that your microphone isn't muted.",
        durationMs: durationMs || 0, wordCount: 0,
        audioFilename, audioBytes
      });
      mainWindow?.webContents.send("history-changed", { id, kind: "update" });
      activeDictationId = null;
      setRecordingState(false);
      if (overlayWindow && !overlayWindow.isDestroyed()) overlayWindow.hide();
      notifyMain("ready", "No speech detected — check that your mic isn't muted.");
      return { ok: false, empty: true };
    }

    const wordCount = transcript.trim().split(/\s+/).filter(Boolean).length;

    historyStore.finalize({
      id, status: "ok", transcript,
      errorKind: null, errorMessage: null,
      durationMs: durationMs || 0, wordCount,
      audioFilename, audioBytes
    });
    mainWindow?.webContents.send("history-changed", { id, kind: "update" });

    notifyMain("working", "Pasting transcript...");
    const insert = await textInserter.insertTranscript(transcript);

    activeDictationId = null;
    setRecordingState(false);
    if (overlayWindow && !overlayWindow.isDestroyed()) overlayWindow.hide();
    const preview = transcript.length > 80 ? transcript.slice(0, 80) + "…" : transcript;
    if (insert.pasted) {
      notifyMain("success", preview);
    } else if (insert.clipboard) {
      notifyMain("success", "Copied to clipboard — press Ctrl+V to paste it.");
      notifyOS("Voxa couldn't auto-paste", "Your transcript is on the clipboard — press Ctrl+V where you want it.");
    } else {
      notifyMain("error", "Couldn't paste or copy — open History to copy it manually.");
      notifyOS("Voxa couldn't deliver the transcript", "Open Voxa → History to copy it.");
    }

    return { ok: true, transcript };
  } catch (error) {
    const msg = error.message || "Unknown error.";
    const errorKind = classifyError(msg);
    log(`finalize-recording error: ${msg} (kind=${errorKind})`);

    try {
      historyStore.finalize({
        id,
        status: (errorKind === "silence" || errorKind === "no_speech") ? "empty" : "failed",
        transcript: null,
        errorKind,
        errorMessage: msg,
        durationMs: durationMs || 0,
        wordCount: 0,
        audioFilename,
        audioBytes
      });
      mainWindow?.webContents.send("history-changed", { id, kind: "update" });
    } catch (err) {
      log(`finalize-recording: failed to record error row: ${err.message}`);
    }

    activeDictationId = null;
    setRecordingState(false);
    if (overlayWindow && !overlayWindow.isDestroyed()) overlayWindow.hide();
    notifyMain("error", msg);
    if (!mainWindow || !mainWindow.isVisible()) notifyExternalError(msg);
    return { ok: false, error: msg, errorKind };
  }
});

// ── History IPC handlers ───────────────────────────────────────────────────
// When the native sqlite module isn't available, history is inert: list/stats
// return empty shapes the renderer can render, mutations report a disabled flag.
ipcMain.handle("history-list", (_e, args) =>
  historyEnabled ? historyStore.list(args || {}) : { rows: [], total: 0 }
);
ipcMain.handle("history-get", (_e, id) => (historyEnabled ? historyStore.get(id) : null));

ipcMain.handle("history-delete", async (_e, id) => {
  if (!historyEnabled) return { ok: false, error: "history_disabled" };
  const row = historyStore.get(id);
  if (row && row.audio_filename) {
    try { await audioStore.deleteFile(id); } catch { /* best effort */ }
  }
  historyStore.deleteOne(id);
  mainWindow?.webContents.send("history-changed", { id, kind: "delete" });
  return { ok: true };
});

ipcMain.handle("history-copy", (_e, id) => {
  if (!historyEnabled) return { ok: false, error: "history_disabled" };
  const row = historyStore.get(id);
  if (row?.transcript) clipboard.writeText(row.transcript);
  return { ok: !!row?.transcript };
});

ipcMain.handle("history-reinsert", async (_e, id) => {
  if (!historyEnabled) return { ok: false, error: "history_disabled" };
  loadServices();
  const row = historyStore.get(id);
  if (!row?.transcript) return { ok: false };
  const insert = await textInserter.insertTranscript(row.transcript);
  return { ok: !!insert.clipboard || !!insert.pasted, pasted: insert.pasted };
});

ipcMain.handle("history-reveal-audio", (_e, id) => {
  if (!historyEnabled) return { ok: false };
  const row = historyStore.get(id);
  if (row?.audio_filename && !row.audio_purged_at) {
    shell.showItemInFolder(audioStore.pathFor(id));
    return { ok: true };
  }
  return { ok: false };
});

// Serve the recorded audio bytes as a base64 data URL so the renderer's drawer
// can play it back. The renderer only ever had the bare filename, which it
// can't load from disk under contextIsolation — hand it the bytes inline.
ipcMain.handle("history-audio-data", (_e, id) => {
  if (!historyEnabled) return null;
  const row = historyStore.get(id);
  if (!row || !row.audio_filename || row.audio_purged_at) return null;
  try {
    const buf = audioStore.readBuffer(id);
    return `data:audio/webm;base64,${buf.toString("base64")}`;
  } catch (err) {
    log(`history-audio-data: ${err.message}`);
    return null;
  }
});

ipcMain.handle("history-stats", (_e, args) =>
  historyEnabled
    ? historyStore.stats(args?.since || (Date.now() - 7 * 24 * 60 * 60 * 1000))
    : { countToday: 0, wordsToday: 0, wordsThisWeek: 0 }
);

ipcMain.handle("history-get-recent-failures", (_e, args) =>
  historyEnabled ? historyStore.getRecentFailures(args?.limit || 5) : []
);

ipcMain.handle("history-retry", async (_e, id) => {
  if (!historyEnabled) return { ok: false, error: "history_disabled" };
  loadServices();
  const row = historyStore.get(id);
  if (!row) return { ok: false, error: "not_found" };
  if (!row.audio_filename || row.audio_purged_at) {
    return { ok: false, error: "audio_unavailable" };
  }
  try {
    const buf = audioStore.readBuffer(id);
    const transcript = await chatgptAuth.transcribeAudio(buf, "audio/webm");
    if (!transcript) throw new Error("Empty transcript returned.");
    const wordCount = transcript.trim().split(/\s+/).filter(Boolean).length;

    // markRetry bumps retry_count + clears the error; finalize re-stamps the
    // transcript, status and word_count (markRetry doesn't touch word_count).
    historyStore.markRetry({ id, status: "ok", transcript, errorKind: null, errorMessage: null });
    historyStore.finalize({
      id, status: "ok", transcript,
      errorKind: null, errorMessage: null,
      durationMs: row.duration_ms, wordCount,
      audioFilename: row.audio_filename, audioBytes: row.audio_bytes
    });

    const insert = await textInserter.insertTranscript(transcript);
    mainWindow?.webContents.send("history-changed", { id, kind: "update" });
    const preview = transcript.length > 80 ? transcript.slice(0, 80) + "…" : transcript;
    if (insert.pasted) {
      notifyMain("success", preview);
    } else if (insert.clipboard) {
      notifyMain("success", "Copied to clipboard — press Ctrl+V to paste it.");
      notifyOS("Voxa couldn't auto-paste", "Your transcript is on the clipboard — press Ctrl+V where you want it.");
    } else {
      notifyMain("error", "Couldn't paste or copy — open History to copy it manually.");
      notifyOS("Voxa couldn't deliver the transcript", "Open Voxa → History to copy it.");
    }
    return { ok: true, transcript };
  } catch (err) {
    const kind = classifyError(err.message);
    historyStore.markRetry({
      id,
      status: (kind === "silence" || kind === "no_speech") ? "empty" : "failed",
      transcript: null,
      errorKind: kind,
      errorMessage: err.message
    });
    mainWindow?.webContents.send("history-changed", { id, kind: "update" });
    return { ok: false, error: err.message, errorKind: kind };
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

ipcMain.handle("get-account-info", async () => {
  loadServices();
  try { return await chatgptAuth.getAccountInfo(); } catch { return null; }
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

ipcMain.handle("get-auto-launch", () => app.getLoginItemSettings({ args: ["--hidden"] }).openAtLogin);
ipcMain.handle("set-auto-launch", (_e, enabled) => { setAutoLaunch(!!enabled); return true; });

ipcMain.handle("sign-out", () => {
  loadServices();
  const ok = chatgptAuth.logout();
  notifyMain(ok ? "ready" : "error", ok ? "Signed out. Sign in again to dictate." : "Could not clear the session.");
  return ok;
});

ipcMain.handle("get-app-version", () => app.getVersion());

ipcMain.handle("open-logs-folder", () => {
  // voxa.log lives directly in userData; reveal it so the file is selected.
  const logPath = path.join(app.getPath("userData"), "voxa.log");
  if (fs.existsSync(logPath)) { shell.showItemInFolder(logPath); return true; }
  return shell.openPath(app.getPath("userData")).then(() => true).catch(() => false);
});

ipcMain.handle("open-data-folder", () => shell.openPath(app.getPath("userData")).then((err) => !err));

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

  // ── History / audio stores + crash recovery ──────────────────────────────
  // Bring up SQLite + the audio directory before anything can record. If the
  // native module never loaded, historyEnabled is false and the whole block is
  // skipped — the core dictation flow runs without persistence.
  if (!historyEnabled) {
    log("history disabled: better-sqlite3 native module not available — skipping history init, crash recovery and retention");
  }
  if (historyEnabled) {
   try {
    historyStore.init();
    audioStore.init();
   } catch (err) {
    log(`history/audio store init failed: ${err.message}`);
    historyEnabled = false;
   }
  }

  // Crash recovery: any row still 'pending' means a previous run died (crashed
  // or was force-quit) mid-dictation. We never got to transcribe it, so mark it
  // failed with a timeout kind and capture whatever audio reached disk.
  if (historyEnabled) try {
    const orphans = historyStore.getOrphanPending();
    if (orphans.length) log(`crash recovery: ${orphans.length} orphan pending dictation(s)`);
    for (const row of orphans) {
      let audioFilename = null;
      let audioBytes = null;
      const size = audioStore.fileSize(row.id);
      if (size != null && size > 0) {
        audioFilename = `${row.id}.webm`;
        audioBytes = size;
      }
      historyStore.finalize({
        id: row.id,
        status: "failed",
        transcript: null,
        errorKind: "timeout",
        errorMessage: "Recording interrupted (app crashed or quit mid-dictation)",
        durationMs: row.duration_ms || 0,
        wordCount: 0,
        audioFilename,
        audioBytes
      });
    }
  } catch (err) {
    log(`crash recovery failed: ${err.message}`);
  }

  // Background audio retention sweep (runs immediately, then every 6h).
  if (historyEnabled) try {
    retentionWorker.start();
  } catch (err) {
    log(`retentionWorker.start failed: ${err.message}`);
  }

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

app.on("before-quit", async (e) => {
  isQuitting = true;

  // Nothing async to do when history is disabled — let the quit proceed.
  if (!historyEnabled) return;

  // Guard against re-entry: we preventDefault on the first pass to run async
  // teardown (privacy purge can hit the disk), then call app.quit() again,
  // which re-fires this handler with purgedOnQuit already true → falls through.
  if (purgedOnQuit) return;
  purgedOnQuit = true;
  e.preventDefault();

  try {
    // Close any audio stream still open for the in-flight dictation so its
    // file handle is released before we (maybe) purge.
    if (activeDictationId) {
      try { await audioStore.close(activeDictationId); } catch { /* ignore */ }
    }

    // Privacy mode: wipe all successful-dictation audio + rows on quit.
    // NB: configStore exposes getPreferences(), not a bare get(); the spec's
    // configStore.get('privacyMode') would be undefined here, so we read the
    // resolved preference instead.
    let privacyMode = false;
    try {
      loadServices();
      privacyMode = !!configStore.getPreferences().privacyMode;
    } catch { /* ignore — default off */ }

    if (privacyMode) {
      log("before-quit: privacy mode on — purging success audio + rows");
      try {
        const result = await retentionWorker.privacyQuitPurge();
        log(`before-quit: privacy purge purgedAudio=${result?.purgedAudio} deletedRows=${result?.deletedRows}`);
        // privacyQuitPurge() marks the success rows purged + deletes them from
        // the DB, but does NOT unlink the on-disk .webm files. Sweep the audio
        // dir for any file no longer referenced by a row and remove it, so the
        // privacy guarantee actually holds on disk.
        const leftover = audioStore.listOrphanFiles();
        let removed = 0;
        for (const filePath of leftover) {
          const base = path.basename(filePath, ".webm");
          // A row that still exists (failed/empty kept for retry) keeps its file.
          if (historyStore.get(base)) continue;
          try { fs.unlinkSync(filePath); removed++; } catch { /* ignore */ }
        }
        if (removed) log(`before-quit: privacy purge unlinked ${removed} orphan audio file(s)`);
      } catch (err) {
        log(`before-quit: privacy purge failed: ${err.message}`);
      }
    }
  } catch (err) {
    log(`before-quit teardown error: ${err.message}`);
  } finally {
    try { retentionWorker.stop(); } catch { /* ignore */ }
    try { historyStore.close(); } catch { /* ignore */ }
    app.quit();
  }
});

app.on("will-quit", () => { globalShortcut.unregisterAll(); });

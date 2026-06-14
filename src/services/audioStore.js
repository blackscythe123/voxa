const fs = require("node:fs");
const path = require("node:path");

// Lazy electron.app so this file can be required before app-ready.
let _app;
function getApp() {
  if (!_app) _app = require("electron").app;
  return _app;
}

function log(...args) {
  process.stdout.write(`[${new Date().toISOString()}] [audioStore] ${args.join(" ")}\n`);
}

function getAudioDir() {
  return path.join(getApp().getPath("userData"), "audio");
}

// Map<id, fs.WriteStream>
const openStreams = new Map();

function init() {
  const dir = getAudioDir();
  try {
    fs.mkdirSync(dir, { recursive: true });
    log("init audio dir:", dir);
  } catch (err) {
    log("init failed:", err && err.message);
    throw err;
  }
}

function pathFor(id) {
  return path.join(getAudioDir(), `${id}.webm`);
}

function open(id) {
  if (openStreams.has(id)) {
    throw new Error(`audioStore.open: id ${id} already open`);
  }
  const p = pathFor(id);
  const stream = fs.createWriteStream(p);
  stream.on("error", (err) => {
    log("stream error for", id, ":", err && err.message);
  });
  openStreams.set(id, stream);
}

function appendChunk(id, buf) {
  const stream = openStreams.get(id);
  if (!stream) {
    log("appendChunk: no open stream for id", id, "(stale chunk ignored)");
    return;
  }
  stream.write(buf);
}

function close(id) {
  const stream = openStreams.get(id);
  if (!stream) {
    return Promise.resolve();
  }
  openStreams.delete(id);
  return new Promise((resolve) => {
    stream.once("close", resolve);
    stream.end();
  });
}

function deleteFile(id) {
  try {
    fs.unlinkSync(pathFor(id));
  } catch (err) {
    if (err && err.code !== "ENOENT") {
      log("deleteFile error for", id, ":", err.message);
    }
  }
}

function readBuffer(id) {
  return fs.readFileSync(pathFor(id));
}

function fileSize(id) {
  try {
    return fs.statSync(pathFor(id)).size;
  } catch {
    return null;
  }
}

// Patches the Duration field inside the WebM SegmentInformation block so
// downstream STT gets an accurate duration (MediaRecorder leaves it 0/missing).
// Scans for EBML element 0x4489 (Duration) with 8-byte float size marker 0x88.
// Ported from src/renderer/overlay.html → injectWebMDuration.
// No-op + warn if the EBML structure doesn't match — file is still playable.
function injectDuration(id, durationMs) {
  return new Promise((resolve) => {
    const p = pathFor(id);
    let buf;
    try {
      buf = fs.readFileSync(p);
    } catch (err) {
      log("injectDuration: read failed for", id, ":", err && err.message);
      resolve();
      return;
    }
    let patched = false;
    const limit = buf.length - 12;
    for (let i = 0; i < limit; i++) {
      if (buf[i] === 0x44 && buf[i + 1] === 0x89 && buf[i + 2] === 0x88) {
        buf.writeDoubleBE(durationMs, i + 3);
        patched = true;
        break;
      }
    }
    if (!patched) {
      log("injectDuration: EBML Duration marker not found for", id, "(skipped)");
      resolve();
      return;
    }
    try {
      fs.writeFileSync(p, buf);
    } catch (err) {
      log("injectDuration: write failed for", id, ":", err && err.message);
    }
    resolve();
  });
}

// Returns absolute paths of .webm files in the audio dir whose basename id
// is NOT currently open. Caller decides which of those are orphans vs. valid
// completed recordings to retain.
function listOrphanFiles() {
  const dir = getAudioDir();
  let entries;
  try {
    entries = fs.readdirSync(dir);
  } catch (err) {
    if (err && err.code === "ENOENT") return [];
    log("listOrphanFiles: readdir failed:", err && err.message);
    return [];
  }
  const result = [];
  for (const name of entries) {
    if (!name.endsWith(".webm")) continue;
    const id = name.slice(0, -".webm".length);
    if (openStreams.has(id)) continue;
    result.push(path.join(dir, name));
  }
  return result;
}

module.exports = {
  init,
  pathFor,
  open,
  appendChunk,
  close,
  deleteFile,
  readBuffer,
  fileSize,
  injectDuration,
  listOrphanFiles
};

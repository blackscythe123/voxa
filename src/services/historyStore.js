// src/services/historyStore.js
//
// History store for Voxa dictations — pure JavaScript, zero native dependencies.
//
// Backed by a single JSON file at <userData>/history.json, loaded into memory on
// init() and persisted atomically (temp-file + rename) after each mutation. This
// replaces the previous better-sqlite3 implementation so the app builds, runs,
// and packages on any machine without Visual Studio / node-gyp build tools.
//
// The public API is unchanged from the SQLite version: identical function names,
// argument shapes, and return values (rows are plain objects with snake_case
// columns; list() returns {rows,total}; stats() returns {countToday,wordsToday,
// wordsThisWeek}). All timestamps are epoch milliseconds.
//
// Path resolution is lazy so this module can be require()'d before app-ready.

const path = require("node:path");
const fs = require("node:fs");

// ---------------------------------------------------------------------------
// Lazy electron.app so this file can be required before app-ready.
// ---------------------------------------------------------------------------
let _app;
function getApp() {
  if (!_app) _app = require("electron").app;
  return _app;
}

function getDbPath() {
  return path.join(getApp().getPath("userData"), "history.json");
}

function log(...args) {
  process.stdout.write(`[${new Date().toISOString()}] [historyStore] ${args.join(" ")}\n`);
}

function logError(...args) {
  process.stderr.write(`[${new Date().toISOString()}] [historyStore] ${args.join(" ")}\n`);
}

// ---------------------------------------------------------------------------
// Module state
// ---------------------------------------------------------------------------
let rows = null;            // in-memory array of dictation rows (newest first not guaranteed)
let byId = null;           // Map<id, row> for O(1) lookup
let dbPath = null;
let opened = false;

const DAY_MS = 24 * 60 * 60 * 1000;
const HOUR_MS = 60 * 60 * 1000;

// Every row carries the full column set (nulls for unset) so consumers can read
// any field without guarding for undefined — matching the old SQLite row shape.
function normalizeRow(r) {
  return {
    id: r.id,
    created_at: Number.isFinite(r.created_at) ? r.created_at : Date.now(),
    duration_ms: Number.isFinite(r.duration_ms) ? r.duration_ms : 0,
    source_app: r.source_app != null ? r.source_app : null,
    source_app_label: r.source_app_label != null ? r.source_app_label : null,
    transcript: r.transcript != null ? r.transcript : null,
    model_id: r.model_id != null ? r.model_id : "chatgpt-backend-transcribe",
    status: r.status,
    error_kind: r.error_kind != null ? r.error_kind : null,
    error_message: r.error_message != null ? r.error_message : null,
    audio_filename: r.audio_filename != null ? r.audio_filename : null,
    audio_purged_at: Number.isFinite(r.audio_purged_at) ? r.audio_purged_at : null,
    audio_bytes: Number.isFinite(r.audio_bytes) ? r.audio_bytes : null,
    word_count: Number.isFinite(r.word_count) ? r.word_count : 0,
    retry_count: Number.isFinite(r.retry_count) ? r.retry_count : 0,
    last_retry_at: Number.isFinite(r.last_retry_at) ? r.last_retry_at : null
  };
}

function clone(r) {
  return r ? { ...r } : r;
}

// ---------------------------------------------------------------------------
// init / close / persist
// ---------------------------------------------------------------------------
function init() {
  if (opened) return; // idempotent

  dbPath = getDbPath();
  rows = [];
  byId = new Map();

  try {
    if (fs.existsSync(dbPath)) {
      const raw = fs.readFileSync(dbPath, "utf8");
      const parsed = JSON.parse(raw);
      if (Array.isArray(parsed)) {
        rows = parsed.map(normalizeRow).filter((r) => r.id != null);
      }
    }
  } catch (err) {
    // A corrupt file shouldn't brick the app — back it up and start fresh.
    logError("init: could not read history.json, starting empty -", err && err.message);
    try {
      if (fs.existsSync(dbPath)) fs.renameSync(dbPath, dbPath + ".corrupt-" + Date.now());
    } catch (_) { /* ignore */ }
    rows = [];
  }

  byId = new Map(rows.map((r) => [r.id, r]));
  opened = true;
  log("init ok:", dbPath, `(${rows.length} rows)`);
}

function close() {
  if (!opened) return;
  try {
    persist();
  } catch (err) {
    logError("close: final persist failed -", err && err.message);
  }
  rows = null;
  byId = null;
  opened = false;
}

function ensureOpen() {
  if (!opened) throw new Error("historyStore: init() must be called before use");
}

// Atomic write: serialise to a temp file then rename over the target.
function persist() {
  if (!opened) return;
  const tmp = dbPath + ".tmp";
  fs.writeFileSync(tmp, JSON.stringify(rows), "utf8");
  fs.renameSync(tmp, dbPath);
}

// ---------------------------------------------------------------------------
// Write methods
// ---------------------------------------------------------------------------
function insertPending({ id, createdAt, sourceApp, sourceAppLabel } = {}) {
  ensureOpen();
  const row = normalizeRow({
    id,
    created_at: createdAt != null ? createdAt : Date.now(),
    source_app: sourceApp,
    source_app_label: sourceAppLabel,
    status: "pending"
  });
  if (byId.has(id)) {
    // Replace any stale row with the same id rather than duplicate.
    const idx = rows.findIndex((r) => r.id === id);
    if (idx >= 0) rows[idx] = row;
  } else {
    rows.push(row);
  }
  byId.set(id, row);
  persist();
}

function finalize({
  id,
  status,
  transcript,
  errorKind,
  errorMessage,
  durationMs,
  wordCount,
  audioFilename,
  audioBytes
} = {}) {
  ensureOpen();
  const row = byId.get(id);
  if (!row) {
    // No pending row (e.g. recovered after a wipe) — create one so nothing is lost.
    const created = normalizeRow({ id, created_at: Date.now(), status });
    rows.push(created);
    byId.set(id, created);
    return finalize({ id, status, transcript, errorKind, errorMessage, durationMs, wordCount, audioFilename, audioBytes });
  }
  row.status = status;
  row.transcript = transcript != null ? transcript : null;
  row.error_kind = errorKind != null ? errorKind : null;
  row.error_message = errorMessage != null ? errorMessage : null;
  row.duration_ms = Number.isFinite(durationMs) ? Math.floor(durationMs) : row.duration_ms;
  row.word_count = Number.isFinite(wordCount) ? Math.floor(wordCount) : row.word_count;
  if (audioFilename !== undefined) row.audio_filename = audioFilename != null ? audioFilename : null;
  if (audioBytes !== undefined) row.audio_bytes = Number.isFinite(audioBytes) ? Math.floor(audioBytes) : row.audio_bytes;
  persist();
}

function markRetry({ id, status, transcript, errorKind, errorMessage } = {}) {
  ensureOpen();
  const row = byId.get(id);
  if (!row) return;
  row.status = status;
  row.transcript = transcript != null ? transcript : null;
  row.error_kind = errorKind != null ? errorKind : null;
  row.error_message = errorMessage != null ? errorMessage : null;
  row.retry_count = (row.retry_count || 0) + 1;
  row.last_retry_at = Date.now();
  persist();
}

// ---------------------------------------------------------------------------
// Read methods
// ---------------------------------------------------------------------------
function list({ limit = 50, offset = 0, search = "", statusFilter = null } = {}) {
  ensureOpen();
  const safeLimit = Number.isFinite(limit) && limit >= 0 ? Math.floor(limit) : 50;
  const safeOffset = Number.isFinite(offset) && offset >= 0 ? Math.floor(offset) : 0;
  const term = typeof search === "string" ? search.trim().toLowerCase() : "";
  const status = statusFilter != null && statusFilter !== "" ? statusFilter : null;

  let filtered = rows;
  if (status) filtered = filtered.filter((r) => r.status === status);
  if (term) {
    filtered = filtered.filter(
      (r) => typeof r.transcript === "string" && r.transcript.toLowerCase().includes(term)
    );
  }

  const total = filtered.length;
  const page = filtered
    .slice()
    .sort((a, b) => b.created_at - a.created_at)
    .slice(safeOffset, safeOffset + safeLimit)
    .map(clone);

  return { rows: page, total };
}

function get(id) {
  ensureOpen();
  return clone(byId.get(id)) || null;
}

// ---------------------------------------------------------------------------
// Delete / purge
// ---------------------------------------------------------------------------
function deleteOne(id) {
  ensureOpen();
  const row = byId.get(id);
  if (!row) return null;
  const idx = rows.findIndex((r) => r.id === id);
  if (idx >= 0) rows.splice(idx, 1);
  byId.delete(id);
  persist();
  return clone(row);
}

function markAudioPurged(id) {
  ensureOpen();
  const row = byId.get(id);
  if (!row) return;
  row.audio_purged_at = Date.now();
  persist();
}

// ---------------------------------------------------------------------------
// Stats
// ---------------------------------------------------------------------------
function stats(sinceMs) {
  ensureOpen();
  const todayStart = Number.isFinite(sinceMs) ? sinceMs : startOfTodayMs();
  const weekStart = todayStart - 6 * DAY_MS;

  let countToday = 0;
  let wordsToday = 0;
  let wordsThisWeek = 0;

  for (const r of rows) {
    if (r.status !== "ok") continue;
    if (r.created_at >= todayStart) {
      countToday += 1;
      wordsToday += r.word_count || 0;
    }
    if (r.created_at >= weekStart) {
      wordsThisWeek += r.word_count || 0;
    }
  }

  return { countToday, wordsToday, wordsThisWeek };
}

// ---------------------------------------------------------------------------
// Crash recovery / retention / privacy
// ---------------------------------------------------------------------------
function getOrphanPending() {
  ensureOpen();
  return rows
    .filter((r) => r.status === "pending")
    .sort((a, b) => a.created_at - b.created_at)
    .map(clone);
}

function getPurgeCandidates({ successHrs, failedDays } = {}) {
  ensureOpen();
  const now = Date.now();
  const hrs = Number.isFinite(successHrs) && successHrs >= 0 ? successHrs : 24;
  const days = Number.isFinite(failedDays) && failedDays >= 0 ? failedDays : 7;
  const successCutoff = now - hrs * HOUR_MS;
  const failedCutoff = now - days * DAY_MS;

  return rows
    .filter((r) => r.audio_filename != null && r.audio_purged_at == null)
    .filter((r) =>
      (r.status === "ok" && r.created_at <= successCutoff) ||
      ((r.status === "failed" || r.status === "empty") && r.created_at <= failedCutoff)
    )
    .map(clone);
}

function getRecentFailures(limit = 5) {
  ensureOpen();
  const safeLimit = Number.isFinite(limit) && limit > 0 ? Math.floor(limit) : 5;
  return rows
    .filter(
      (r) =>
        (r.status === "failed" || r.status === "empty") &&
        r.audio_filename != null &&
        r.audio_purged_at == null
    )
    .sort((a, b) => b.created_at - a.created_at)
    .slice(0, safeLimit)
    .map(clone);
}

function purgeAllSuccessAudio() {
  ensureOpen();
  const now = Date.now();
  const snapshot = [];
  for (const r of rows) {
    if (r.status === "ok" && r.audio_filename != null && r.audio_purged_at == null) {
      snapshot.push({ id: r.id, audio_filename: r.audio_filename });
      r.audio_purged_at = now;
    }
  }
  persist();
  return snapshot; // [{ id, audio_filename }, ...] — caller deletes the files.
}

function deleteAllSuccess() {
  ensureOpen();
  const before = rows.length;
  rows = rows.filter((r) => r.status !== "ok");
  byId = new Map(rows.map((r) => [r.id, r]));
  persist();
  return before - rows.length; // affected count
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------
function startOfTodayMs() {
  const d = new Date();
  d.setHours(0, 0, 0, 0);
  return d.getTime();
}

module.exports = {
  init,
  close,
  insertPending,
  finalize,
  markRetry,
  list,
  get,
  deleteOne,
  markAudioPurged,
  stats,
  getOrphanPending,
  getPurgeCandidates,
  getRecentFailures,
  purgeAllSuccessAudio,
  deleteAllSuccess
};

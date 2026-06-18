const historyStore = require("./historyStore");
const audioStore = require("./audioStore");
const configStore = require("./configStore");

const SIX_HOURS_MS = 6 * 60 * 60 * 1000;

const DEFAULT_SUCCESS_HOURS = 0; // keep successful audio by default
const DEFAULT_FAILED_DAYS = 7;
const DEFAULT_AUDIO_MAX_MB = 2048;

let intervalHandle = null;
let started = false;
let running = false;

function readPrefs() {
  let prefs = {};
  try {
    prefs = configStore.getPreferences() || {};
  } catch (err) {
    log("warn", "failed to read preferences, using defaults", err);
  }
  const successAudioRetentionHours = numberOrDefault(
    prefs.successAudioRetentionHours,
    DEFAULT_SUCCESS_HOURS
  );
  const failedAudioRetentionDays = numberOrDefault(
    prefs.failedAudioRetentionDays,
    DEFAULT_FAILED_DAYS
  );
  const audioMaxMB = numberOrDefault(prefs.audioMaxMB, DEFAULT_AUDIO_MAX_MB);
  const privacyMode = Boolean(prefs.privacyMode);
  return { successAudioRetentionHours, failedAudioRetentionDays, audioMaxMB, privacyMode };
}

function numberOrDefault(value, fallback) {
  const n = Number(value);
  if (!Number.isFinite(n) || n < 0) return fallback;
  return n;
}

function log(level, ...args) {
  const fn = console[level] || console.log;
  fn.call(console, "[retentionWorker]", ...args);
}

async function runOnce() {
  if (running) {
    log("log", "skip: previous run still in progress");
    return { purgedCount: 0, errors: [] };
  }
  running = true;
  const errors = [];
  let purgedCount = 0;
  try {
    const prefs = readPrefs();
    let candidates = [];
    try {
      candidates = historyStore.getPurgeCandidates({
        successHrs: prefs.successAudioRetentionHours,
        failedDays: prefs.failedAudioRetentionDays
      }) || [];
    } catch (err) {
      log("error", "getPurgeCandidates failed", err);
      return { purgedCount: 0, errors: [{ id: null, error: String(err && err.message || err) }] };
    }

    log(
      "log",
      `run start: candidates=${candidates.length} successHrs=${prefs.successAudioRetentionHours} failedDays=${prefs.failedAudioRetentionDays}`
    );

    for (const row of candidates) {
      if (!row || row.id == null) continue;
      try {
        await audioStore.deleteFile(row.id);
        historyStore.markAudioPurged(row.id);
        purgedCount += 1;
      } catch (err) {
        const message = String((err && err.message) || err);
        errors.push({ id: row.id, error: message });
        log("warn", `purge failed for id=${row.id}: ${message}`);
      }
    }

    // Disk-cap purge: keep total success audio under audioMaxMB by evicting the
    // oldest first. This is what bounds disk now that success audio is kept.
    try {
      const capBytes = prefs.audioMaxMB > 0 ? prefs.audioMaxMB * 1024 * 1024 : Infinity;
      if (Number.isFinite(capBytes)) {
        const successRows = historyStore.getSuccessAudioSorted() || []; // oldest first
        let total = successRows.reduce((s, r) => s + (r.audio_bytes || 0), 0);
        let evicted = 0;
        for (const r of successRows) {
          if (total <= capBytes) break;
          try {
            await audioStore.deleteFile(r.id);
            historyStore.markAudioPurged(r.id);
            total -= r.audio_bytes || 0;
            purgedCount += 1;
            evicted += 1;
          } catch (err) {
            errors.push({ id: r.id, error: String((err && err.message) || err) });
          }
        }
        if (evicted > 0) {
          log("log", `disk-cap: evicted ${evicted} oldest clips to stay under ${prefs.audioMaxMB}MB`);
        }
      }
    } catch (err) {
      log("error", "disk-cap purge failed", err);
    }

    log("log", `run end: purged=${purgedCount} errors=${errors.length}`);
    return { purgedCount, errors };
  } finally {
    running = false;
  }
}

function start() {
  if (started) {
    return;
  }
  started = true;
  // Fire-and-forget the immediate run; surface errors via logs only.
  Promise.resolve()
    .then(() => runOnce())
    .catch((err) => log("error", "initial runOnce threw", err));
  intervalHandle = setInterval(() => {
    runOnce().catch((err) => log("error", "scheduled runOnce threw", err));
  }, SIX_HOURS_MS);
  if (intervalHandle && typeof intervalHandle.unref === "function") {
    intervalHandle.unref();
  }
}

function stop() {
  if (intervalHandle) {
    clearInterval(intervalHandle);
    intervalHandle = null;
  }
  started = false;
}

async function privacyQuitPurge() {
  const prefs = readPrefs();
  const result = { purgedAudio: 0, deletedRows: 0, errors: [] };
  if (!prefs.privacyMode) {
    return result;
  }
  try {
    const purged = historyStore.purgeAllSuccessAudio();
    result.purgedAudio = typeof purged === "number" ? purged : (purged && purged.count) || 0;
  } catch (err) {
    const message = String((err && err.message) || err);
    result.errors.push({ stage: "purgeAllSuccessAudio", error: message });
    log("error", `privacyQuitPurge purgeAllSuccessAudio failed: ${message}`);
  }
  try {
    const deleted = historyStore.deleteAllSuccess();
    result.deletedRows = typeof deleted === "number" ? deleted : (deleted && deleted.count) || 0;
  } catch (err) {
    const message = String((err && err.message) || err);
    result.errors.push({ stage: "deleteAllSuccess", error: message });
    log("error", `privacyQuitPurge deleteAllSuccess failed: ${message}`);
  }
  log(
    "log",
    `privacyQuitPurge: purgedAudio=${result.purgedAudio} deletedRows=${result.deletedRows} errors=${result.errors.length}`
  );
  return result;
}

module.exports = {
  start,
  stop,
  runOnce,
  privacyQuitPurge
};

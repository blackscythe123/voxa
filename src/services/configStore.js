const Store = require("electron-store");

const defaults = {
  startHotkey: "F9",
  stopHotkey: "Escape",
  overlayPosition: "bottom-right",
  maxRecordingSeconds: 300,
  inputDeviceId: "default",
  successAudioRetentionHours: 24,
  failedAudioRetentionDays: 7,
  historyMaxEntries: 0,
  privacyMode: false,
  showLastFailedSection: true,
  lastFailedBannerCap: 5
};

const numericMinZeroKeys = [
  "successAudioRetentionHours",
  "failedAudioRetentionDays",
  "historyMaxEntries",
  "lastFailedBannerCap"
];

function coerceNumericMinZero(key, value) {
  const n = Number(value);
  if (!Number.isFinite(n) || n < 0) {
    return defaults[key];
  }
  return Math.floor(n);
}

const store = new Store({ name: "preferences", defaults });

function getPreferences() {
  return {
    startHotkey: store.get("startHotkey"),
    stopHotkey: store.get("stopHotkey"),
    overlayPosition: store.get("overlayPosition"),
    maxRecordingSeconds: store.get("maxRecordingSeconds"),
    inputDeviceId: store.get("inputDeviceId"),
    successAudioRetentionHours: store.get("successAudioRetentionHours"),
    failedAudioRetentionDays: store.get("failedAudioRetentionDays"),
    historyMaxEntries: store.get("historyMaxEntries"),
    privacyMode: store.get("privacyMode"),
    showLastFailedSection: store.get("showLastFailedSection"),
    lastFailedBannerCap: store.get("lastFailedBannerCap")
  };
}

function savePreferences(next) {
  Object.keys(defaults).forEach((key) => {
    if (Object.prototype.hasOwnProperty.call(next, key)) {
      let value = next[key];
      if (numericMinZeroKeys.includes(key)) {
        value = coerceNumericMinZero(key, value);
      } else if (typeof defaults[key] === "boolean") {
        value = Boolean(value);
      }
      store.set(key, value);
    }
  });
  return getPreferences();
}

module.exports = { getPreferences, savePreferences };

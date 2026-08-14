const Store = require("electron-store");

// F9 collides with macOS's reserved Mission Control / App Exposé "show
// application windows" shortcut on many keyboard configs — pressing it can
// bring other apps' windows forward alongside the overlay. Use a combo
// that's free on both platforms as the mac default instead.
const isMac = process.platform === "darwin";

const defaults = {
  startHotkey: isMac ? "Alt+Space" : "F9",
  stopHotkey: "Escape",
  overlayPosition: "bottom-right",
  maxRecordingSeconds: 300,
  inputDeviceId: "default",
  // 0 = keep successful audio (so past dictations stay playable). Disk use is
  // bounded by audioMaxMB, which evicts the oldest audio when exceeded.
  successAudioRetentionHours: 0,
  failedAudioRetentionDays: 7,
  audioMaxMB: 2048,
  historyMaxEntries: 0,
  privacyMode: false,
  showLastFailedSection: true,
  lastFailedBannerCap: 5
};

const numericMinZeroKeys = [
  "successAudioRetentionHours",
  "failedAudioRetentionDays",
  "audioMaxMB",
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

// One-time upgrade: earlier builds defaulted success-audio retention to 24h,
// which silently deleted past audio. Now we keep it. Flip the old default to
// "keep" once, so existing users get playable history without digging into
// Settings. A user who deliberately set a non-24 value is left untouched.
if (!store.get("retentionMigratedV2") && store.get("successAudioRetentionHours") === 24) {
  store.set("successAudioRetentionHours", 0);
}
store.set("retentionMigratedV2", true);

// One-time upgrade: move existing mac users off prior mac defaults (F9 —
// collides with Mission Control; Alt+Shift+V — an earlier attempt at a safe
// default that wasn't well liked) onto the current default, unless they've
// since customized it themselves.
if (isMac && !store.get("macHotkeyMigratedV2") && ["F9", "Alt+Shift+V"].includes(store.get("startHotkey"))) {
  store.set("startHotkey", "Alt+Space");
}
store.set("macHotkeyMigratedV2", true);

function getPreferences() {
  return {
    startHotkey: store.get("startHotkey"),
    stopHotkey: store.get("stopHotkey"),
    overlayPosition: store.get("overlayPosition"),
    maxRecordingSeconds: store.get("maxRecordingSeconds"),
    inputDeviceId: store.get("inputDeviceId"),
    successAudioRetentionHours: store.get("successAudioRetentionHours"),
    failedAudioRetentionDays: store.get("failedAudioRetentionDays"),
    audioMaxMB: store.get("audioMaxMB"),
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

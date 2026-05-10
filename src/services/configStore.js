const Store = require("electron-store");

const defaults = {
  startHotkey: "F9",
  stopHotkey: "Escape",
  overlayPosition: "bottom-right",
  maxRecordingSeconds: 120,
  inputDeviceId: "default"
};

const store = new Store({ name: "preferences", defaults });

function getPreferences() {
  return {
    startHotkey: store.get("startHotkey"),
    stopHotkey: store.get("stopHotkey"),
    overlayPosition: store.get("overlayPosition"),
    maxRecordingSeconds: store.get("maxRecordingSeconds"),
    inputDeviceId: store.get("inputDeviceId")
  };
}

function savePreferences(next) {
  Object.keys(defaults).forEach((key) => {
    if (Object.prototype.hasOwnProperty.call(next, key)) {
      store.set(key, next[key]);
    }
  });
  return getPreferences();
}

module.exports = { getPreferences, savePreferences };

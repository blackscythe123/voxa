const Store = require("electron-store");

const defaults = {
  startHotkey: "CommandOrControl+Shift+R",
  stopHotkey: "Escape",
  overlayPosition: "bottom-right",
  autoLoginOnStart: false,
  maxRecordingSeconds: 120
};

const store = new Store({
  name: "preferences",
  defaults
});

function getPreferences() {
  return {
    startHotkey: store.get("startHotkey"),
    stopHotkey: store.get("stopHotkey"),
    overlayPosition: store.get("overlayPosition"),
    autoLoginOnStart: store.get("autoLoginOnStart"),
    maxRecordingSeconds: store.get("maxRecordingSeconds")
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

module.exports = {
  getPreferences,
  savePreferences
};

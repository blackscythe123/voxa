// src/services/acceleratorLabel.js
//
// Formats an Electron accelerator string (e.g. "CommandOrControl+Shift+D")
// into a human-readable label for display, per-platform. Electron's
// globalShortcut.register() already maps "CommandOrControl" to the right
// physical key per OS — this only affects what we *show* the user.

function formatAcceleratorLabel(accel, platform = process.platform) {
  if (!accel) return "";
  const isMac = platform === "darwin";
  return accel
    .split("+")
    .map((token) => {
      if (token === "CommandOrControl") return isMac ? "Cmd" : "Ctrl";
      if (token === "Super") return isMac ? "Cmd" : "Win";
      if (token === "Alt") return isMac ? "Option" : "Alt";
      return token;
    })
    .join("+");
}

module.exports = { formatAcceleratorLabel };

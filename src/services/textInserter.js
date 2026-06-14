const { clipboard } = require("electron");
const { exec } = require("node:child_process");

// Insert transcribed text into the foreground window.
//
// Strategy: write to the clipboard (the guaranteed fallback the user can
// always paste manually), then attempt an automatic Ctrl+V via SendKeys on
// Windows. This function NEVER throws — callers rely on the returned object
// to decide whether to show a "pasted" vs "copied, paste manually" toast.
//
// Returns: { pasted: boolean, clipboard: boolean, empty?: boolean, error?: string }
//   pasted    — Ctrl+V was dispatched successfully
//   clipboard — the text was confirmed present on the clipboard
//   empty     — input was blank/whitespace; nothing was done
//   error     — present when the paste attempt failed (timeout/exec error)
async function insertTranscript(text) {
  if (!text || !text.trim()) {
    return { pasted: false, clipboard: false, empty: true };
  }

  // The guaranteed fallback: put the text on the clipboard, then verify it
  // actually landed (clipboard.writeText can silently no-op in rare cases).
  let clipboardOk = false;
  try {
    clipboard.writeText(text);
    clipboardOk = clipboard.readText() === text;
  } catch {
    clipboardOk = false;
  }

  // Give the foreground window time to settle after the overlay hides.
  await new Promise((r) => setTimeout(r, 200));

  // Only attempt the SendKeys paste on Windows. Elsewhere, the clipboard
  // fallback is all we can offer.
  if (process.platform !== "win32") {
    return { pasted: false, clipboard: clipboardOk };
  }

  return await new Promise((resolve) => {
    // -WindowStyle Hidden suppresses the PowerShell console flash.
    exec(
      "powershell -NoProfile -NonInteractive -WindowStyle Hidden -Command \"Add-Type -AssemblyName System.Windows.Forms; [System.Windows.Forms.SendKeys]::SendWait('^v')\"",
      { timeout: 5000 },
      (err) => {
        if (err) {
          resolve({ pasted: false, clipboard: clipboardOk, error: err.message });
        } else {
          resolve({ pasted: true, clipboard: clipboardOk });
        }
      }
    );
  });
}

module.exports = { insertTranscript };

const { clipboard } = require("electron");
const { exec } = require("node:child_process");

async function insertTranscript(text) {
  if (!text || !text.trim()) return;

  clipboard.writeText(text);

  // Give the foreground window time to settle after the overlay hides.
  await new Promise((r) => setTimeout(r, 200));

  await new Promise((resolve, reject) => {
    // -WindowStyle Hidden suppresses the PowerShell console flash.
    exec(
      "powershell -NoProfile -NonInteractive -WindowStyle Hidden -Command \"Add-Type -AssemblyName System.Windows.Forms; [System.Windows.Forms.SendKeys]::SendWait('^v')\"",
      { timeout: 5000 },
      (err) => {
        if (err) reject(err);
        else resolve();
      }
    );
  });
}

module.exports = { insertTranscript };

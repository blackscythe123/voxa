const { clipboard } = require("electron");
const { exec } = require("node:child_process");

async function insertTranscript(text) {
  if (!text || !text.trim()) return;

  clipboard.writeText(text);

  await new Promise((r) => setTimeout(r, 80));

  await new Promise((resolve, reject) => {
    exec(
      "powershell -NoProfile -NonInteractive -Command \"Add-Type -AssemblyName System.Windows.Forms; [System.Windows.Forms.SendKeys]::SendWait('^v')\"",
      (err) => {
        if (err) reject(err);
        else resolve();
      }
    );
  });
}

module.exports = { insertTranscript };

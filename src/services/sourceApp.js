// src/services/sourceApp.js
//
// Captures the foreground application name on Windows and macOS so each
// dictation can be tagged with which app received it (e.g. "VS Code", "Slack").
//
// Public API:
//   getForegroundAppLabel() -> Promise<{ exe: string|null, label: string|null }>
//
// Notes:
//   - On platforms other than win32/darwin, resolves to { exe: null, label: null } immediately.
//   - The PowerShell/osascript call is bounded to ~1.5s; any timeout/error yields
//     { exe: null, label: null } so we never break a dictation just because we
//     couldn't identify the app.

const { exec } = require('child_process');

const POWERSHELL_TIMEOUT_MS = 1500;
const OSASCRIPT_TIMEOUT_MS = 1500;

// Case-insensitive lookup on exe basename without the .exe suffix.
const LABELS = {
  'code': 'VS Code',
  'cursor': 'Cursor',
  'slack': 'Slack',
  'discord': 'Discord',
  'chrome': 'Chrome',
  'msedge': 'Edge',
  'firefox': 'Firefox',
  'notepad': 'Notepad',
  'notepad++': 'Notepad++',
  'explorer': 'File Explorer',
  'winword': 'Word',
  'excel': 'Excel',
  'outlook': 'Outlook',
  'powerpnt': 'PowerPoint',
  'teams': 'Teams',
  'whatsapp': 'WhatsApp',
  'telegram': 'Telegram',
  'electron': 'Electron app',
  'voxa': 'Voxa',
};

// PowerShell snippet that prints the foreground process's ProcessName (no extension).
// We use a single-quoted here-string (@'...'@) for the inline C# so the embedded
// double quotes survive verbatim, and we feed the whole script to PowerShell over
// stdin (`-Command -`) rather than `-Command "..."`. That matters: a here-string
// header (@') must be the LAST thing on its line, so the lines MUST be joined with
// real newlines, not "; " — and stdin avoids any double-quote/semicolon escaping.
const PS_SCRIPT = [
  "Add-Type @'",
  'using System;',
  'using System.Runtime.InteropServices;',
  'using System.Diagnostics;',
  'public class FG {',
  '  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();',
  '  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr hWnd, out uint lpdwProcessId);',
  '}',
  "'@",
  '$h = [FG]::GetForegroundWindow()',
  '$procId = 0',
  '[FG]::GetWindowThreadProcessId($h, [ref]$procId) | Out-Null',
  '$p = Get-Process -Id $procId -ErrorAction SilentlyContinue',
  'if ($p) { Write-Output $p.ProcessName } else { Write-Output "" }',
].join('\n');

// macOS frontmost-process names, as System Events reports them, mapped to
// friendly labels. Keyed by exact case (mac app names are meaningful case,
// unlike Windows exe basenames).
const LABELS_MAC = {
  'Code': 'VS Code',
  'Cursor': 'Cursor',
  'Slack': 'Slack',
  'Discord': 'Discord',
  'Google Chrome': 'Chrome',
  'Safari': 'Safari',
  'Firefox': 'Firefox',
  'Microsoft Word': 'Word',
  'Microsoft Excel': 'Excel',
  'Microsoft Outlook': 'Outlook',
  'Microsoft PowerPoint': 'PowerPoint',
  'Microsoft Teams': 'Teams',
  'WhatsApp': 'WhatsApp',
  'Telegram': 'Telegram',
  'TextEdit': 'TextEdit',
  'Finder': 'Finder',
  'Terminal': 'Terminal',
  'iTerm2': 'iTerm',
  'Notes': 'Notes',
  'Electron': 'Electron app',
  'Voxa': 'Voxa',
};

function normalizeExe(raw) {
  if (!raw) return null;
  let name = String(raw).trim();
  if (!name) return null;
  // Strip a trailing .exe if PowerShell ever surfaces one (ProcessName usually doesn't).
  name = name.replace(/\.exe$/i, '');
  return name || null;
}

function labelFor(exe) {
  if (!exe) return null;
  const key = exe.toLowerCase();
  return LABELS[key] || exe; // fall back to the raw exe name if we don't have a friendly label
}

// Mac app names are meaningful mixed-case (e.g. "Google Chrome"), so this
// looks up LABELS_MAC directly rather than lowercasing like labelFor above.
function labelForMac(name) {
  if (!name) return null;
  return LABELS_MAC[name] || name;
}

function runPowerShell() {
  return new Promise((resolve) => {
    // `-Command -` reads the script from stdin, which keeps the here-string intact
    // (a here-string header must end its line) and sidesteps all quote escaping.
    const cmd = 'powershell -NoProfile -ExecutionPolicy Bypass -Command -';
    let settled = false;
    const child = exec(
      cmd,
      { timeout: POWERSHELL_TIMEOUT_MS, windowsHide: true },
      (err, stdout) => {
        if (settled) return;
        settled = true;
        if (err) {
          resolve(null);
          return;
        }
        const first = String(stdout || '').split(/\r?\n/).find((l) => l.trim().length > 0);
        resolve(first ? first.trim() : null);
      }
    );
    // Pipe the script in and close stdin so PowerShell starts executing.
    try {
      child.stdin.end(PS_SCRIPT + '\n');
    } catch (_) {
      // If stdin isn't writable for some reason, let the timeout/err path handle it.
    }
    // Belt-and-braces: if exec's own timeout doesn't fire, force-resolve.
    setTimeout(() => {
      if (settled) return;
      settled = true;
      try { child.kill(); } catch (_) { /* noop */ }
      resolve(null);
    }, POWERSHELL_TIMEOUT_MS + 200);
  });
}

// AppleScript one-liner asking System Events for the frontmost process name.
// No Accessibility permission required for this query — just Automation/AppleEvents.
const OSASCRIPT_CMD =
  'osascript -e \'tell application "System Events" to get name of first application process whose frontmost is true\'';

function runOsascript() {
  return new Promise((resolve) => {
    let settled = false;
    const child = exec(
      OSASCRIPT_CMD,
      { timeout: OSASCRIPT_TIMEOUT_MS },
      (err, stdout) => {
        if (settled) return;
        settled = true;
        if (err) {
          resolve(null);
          return;
        }
        const name = String(stdout || '').trim();
        resolve(name || null);
      }
    );
    // Belt-and-braces: if exec's own timeout doesn't fire, force-resolve.
    setTimeout(() => {
      if (settled) return;
      settled = true;
      try { child.kill(); } catch (_) { /* noop */ }
      resolve(null);
    }, OSASCRIPT_TIMEOUT_MS + 200);
  });
}

async function getForegroundAppLabel() {
  if (process.platform === 'win32') {
    try {
      const raw = await runPowerShell();
      const exe = normalizeExe(raw);
      if (!exe) return { exe: null, label: null };
      return { exe, label: labelFor(exe) };
    } catch (_) {
      return { exe: null, label: null };
    }
  }

  if (process.platform === 'darwin') {
    try {
      const name = await runOsascript();
      if (!name) return { exe: null, label: null };
      return { exe: name, label: labelForMac(name) };
    } catch (_) {
      return { exe: null, label: null };
    }
  }

  return { exe: null, label: null };
}

module.exports = { getForegroundAppLabel };

const fs = require("node:fs");
const path = require("node:path");
const { app, shell } = require("electron");
const { request } = require("playwright");

const STORAGE_STATE_NAME = "chatgpt-storage-state.json";
const MANUAL_SESSION_NAME = "chatgpt-manual-session.json";
const CHATGPT_LOGIN_URL = "https://chatgpt.com/auth/login";
const CHATGPT_SESSION_URL = "https://chatgpt.com/api/auth/session";

function getStorageStatePath() {
  return path.join(app.getPath("userData"), STORAGE_STATE_NAME);
}

function getManualSessionPath() {
  return path.join(app.getPath("userData"), MANUAL_SESSION_NAME);
}

function hasStorageState() {
  return fs.existsSync(getStorageStatePath());
}

function hasManualSession() {
  return fs.existsSync(getManualSessionPath());
}

function parseSessionText(rawText) {
  const text = String(rawText || "").trim();
  const jsonStart = text.indexOf("{");
  const jsonEnd = text.lastIndexOf("}");

  if (jsonStart === -1 || jsonEnd === -1 || jsonEnd <= jsonStart) {
    throw new Error("Session text must contain JSON from /api/auth/session.");
  }

  const jsonText = text.slice(jsonStart, jsonEnd + 1);
  return JSON.parse(jsonText);
}

function saveManualSessionFromText(rawText) {
  const payload = parseSessionText(rawText);
  const accessToken = payload?.accessToken;

  if (!accessToken || typeof accessToken !== "string") {
    throw new Error("No accessToken found. If only WARNING_BANNER is visible, login first.");
  }

  const expires = payload?.expires || null;
  const record = {
    savedAt: new Date().toISOString(),
    accessToken,
    expires,
    user: payload?.user || null
  };

  fs.writeFileSync(getManualSessionPath(), JSON.stringify(record, null, 2), "utf8");
  return { ok: true, hasUser: Boolean(record.user), expires: record.expires };
}

function readManualSession() {
  if (!hasManualSession()) {
    return null;
  }

  try {
    const raw = fs.readFileSync(getManualSessionPath(), "utf8");
    const parsed = JSON.parse(raw);
    if (!parsed?.accessToken) {
      return null;
    }
    return parsed;
  } catch {
    return null;
  }
}

function isManualSessionValid(record) {
  if (!record?.accessToken) {
    return false;
  }

  if (!record.expires) {
    return true;
  }

  const expiryMs = Date.parse(record.expires);
  if (Number.isNaN(expiryMs)) {
    return true;
  }

  return expiryMs > Date.now() + 30_000;
}

function hasUsableAuth() {
  const manual = readManualSession();
  return hasStorageState() || isManualSessionValid(manual);
}

async function openSessionEndpoint() {
  await shell.openExternal(CHATGPT_SESSION_URL);
}

async function openLoginPage() {
  await shell.openExternal(CHATGPT_LOGIN_URL);
}

async function loginWithPlaywright() {
  try {
    // Step 1: open /api/auth/session directly in the real browser.
    await openSessionEndpoint();
    return hasUsableAuth();
  } catch (error) {
    console.error("System browser login launch failed:", error.message);
    return false;
  }
}

async function transcribeAudio(audioBuffer, mimeType = "audio/webm") {
  const manual = readManualSession();
  const useManualToken = isManualSessionValid(manual);

  if (!hasStorageState() && !useManualToken) {
    throw new Error("Not logged in. Open /api/auth/session, then paste full JSON into the app.");
  }

  const api = await request.newContext(
    useManualToken
      ? {
          extraHTTPHeaders: {
            authorization: `Bearer ${manual.accessToken}`,
            origin: "https://chatgpt.com",
            referer: "https://chatgpt.com/"
          }
        }
      : {
          storageState: getStorageStatePath(),
          extraHTTPHeaders: {
            origin: "https://chatgpt.com",
            referer: "https://chatgpt.com/"
          }
        }
  );

  try {
    const formData = new FormData();
    formData.append("file", new Blob([audioBuffer], { type: mimeType }), "recording.webm");
    formData.append("model", "whisper-1");

    const response = await api.post("https://chatgpt.com/backend-api/transcribe", {
      multipart: {
        file: {
          name: "recording.webm",
          mimeType,
          buffer: Buffer.from(audioBuffer)
        },
        model: "whisper-1"
      },
      timeout: 120000
    });

    if (!response.ok()) {
      const errorText = await response.text();
      throw new Error(`Transcription failed (${response.status()}): ${errorText.slice(0, 300)}`);
    }

    const payload = await response.json();
    return payload.text || payload.transcript || payload.message || "";
  } finally {
    await api.dispose();
  }
}

module.exports = {
  hasStorageState,
  hasUsableAuth,
  saveManualSessionFromText,
  openSessionEndpoint,
  openLoginPage,
  loginWithPlaywright,
  transcribeAudio
};

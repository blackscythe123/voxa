const fs = require("node:fs");
const path = require("node:path");
const { app } = require("electron");
const { chromium, request } = require("playwright");

const STORAGE_STATE_NAME = "chatgpt-storage-state.json";

function getStorageStatePath() {
  return path.join(app.getPath("userData"), STORAGE_STATE_NAME);
}

function hasStorageState() {
  return fs.existsSync(getStorageStatePath());
}

async function loginWithPlaywright() {
  const browser = await chromium.launch({
    headless: false,
    channel: "chrome"
  });

  const context = await browser.newContext();
  const page = await context.newPage();
  await page.goto("https://chatgpt.com/auth/login", { waitUntil: "domcontentloaded" });

  const startedAt = Date.now();
  const timeoutMs = 5 * 60 * 1000;

  while (Date.now() - startedAt < timeoutMs) {
    const cookies = await context.cookies("https://chatgpt.com");
    const hasSessionCookie = cookies.some((cookie) =>
      cookie.name.toLowerCase().includes("session") ||
      cookie.name.toLowerCase().includes("auth")
    );

    if (hasSessionCookie) {
      await context.storageState({ path: getStorageStatePath() });
      await browser.close();
      return true;
    }

    await new Promise((resolve) => setTimeout(resolve, 1000));
  }

  await browser.close();
  return false;
}

async function transcribeAudio(audioBuffer, mimeType = "audio/webm") {
  if (!hasStorageState()) {
    throw new Error("Not logged in to ChatGPT yet. Please run login first.");
  }

  const api = await request.newContext({
    storageState: getStorageStatePath(),
    extraHTTPHeaders: {
      origin: "https://chatgpt.com",
      referer: "https://chatgpt.com/"
    }
  });

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
  loginWithPlaywright,
  transcribeAudio
};

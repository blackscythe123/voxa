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

async function verifyAuthenticatedSession(context) {
  try {
    const response = await context.request.get("https://chatgpt.com/backend-api/models", {
      timeout: 10000,
      failOnStatusCode: false
    });

    return response.status() === 200;
  } catch {
    return false;
  }
}

async function loginWithPlaywright() {
  const browser = await chromium.launch({
    headless: false,
    channel: "chrome"
  });

  try {
    const context = await browser.newContext();
    const page = await context.newPage();
    
    // Open ChatGPT and wait until backend session is truly authenticated.
    await page.goto("https://chatgpt.com", { waitUntil: "domcontentloaded" });
    
    console.log("Waiting for ChatGPT login...");
    const startedAt = Date.now();
    const timeoutMs = 10 * 60 * 1000; // 10 minutes to allow user to complete login

    // Poll for successful authentication.
    while (Date.now() - startedAt < timeoutMs) {
      try {
        const isAuthenticated = await verifyAuthenticatedSession(context);
        if (isAuthenticated) {
          console.log("ChatGPT login detected (backend session verified).");
          await context.storageState({ path: getStorageStatePath() });
          await browser.close();
          return true;
        }
      } catch (e) {
        // Page might have changed during check, retry
      }

      // Check every 2 seconds
      await new Promise((resolve) => setTimeout(resolve, 2000));
    }

    console.log("ChatGPT login timed out.");
    await browser.close();
    return false;
  } catch (error) {
    console.error("Login error:", error.message);
    try {
      await browser.close();
    } catch (e) {
      // Ignore close errors
    }
    return false;
  }
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

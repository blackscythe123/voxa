const fs = require("node:fs");
const path = require("node:path");
const { randomUUID } = require("node:crypto");
const { app, BrowserWindow, session, net } = require("electron");

const ICON_PATH = path.join(__dirname, "..", "..", "assets", "icon.png");

const COOKIES_FILE = "chatgpt-cookies.json";
const DEVICE_ID_FILE = "chatgpt-device-id.txt";
const SESSION_HOSTNAME = "chatgpt.com";
const OAI_SESSION_ID = randomUUID();
const SESSION_URL = "https://chatgpt.com/api/auth/session";
const LOGIN_URL = "https://chatgpt.com/auth/login";
const TRANSCRIBE_URL = "https://chatgpt.com/backend-api/transcribe";

function getCookiesPath() {
  return path.join(app.getPath("userData"), COOKIES_FILE);
}

function getDeviceId() {
  const p = path.join(app.getPath("userData"), DEVICE_ID_FILE);
  if (fs.existsSync(p)) return fs.readFileSync(p, "utf8").trim();
  const id = randomUUID();
  fs.writeFileSync(p, id, "utf8");
  return id;
}

function hasCookies() {
  return fs.existsSync(getCookiesPath());
}

function loadCookies() {
  try {
    return JSON.parse(fs.readFileSync(getCookiesPath(), "utf8"));
  } catch {
    return [];
  }
}

function saveCookies(cookies) {
  fs.writeFileSync(getCookiesPath(), JSON.stringify(cookies, null, 2), "utf8");
}

function buildCookieHeader(cookies) {
  return cookies.map((c) => `${c.name}=${c.value}`).join("; ");
}

function browserHeaders(cookies, extra = {}) {
  return {
    Cookie: buildCookieHeader(cookies),
    "User-Agent": "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Safari/537.36",
    Accept: "*/*",
    "Accept-Language": "en-US,en;q=0.5",
    "Cache-Control": "no-cache",
    Pragma: "no-cache",
    Origin: "https://chatgpt.com",
    Referer: "https://chatgpt.com/",
    "oai-language": "en-US",
    "oai-device-id": getDeviceId(),
    "oai-session-id": OAI_SESSION_ID,
    ...extra
  };
}

// Uses electron.net — Chromium TLS stack, passes Cloudflare fingerprint checks
function netGet(url, headers) {
  return new Promise((resolve, reject) => {
    const req = net.request({ method: "GET", url });
    for (const [k, v] of Object.entries(headers)) req.setHeader(k, String(v));
    const chunks = [];
    req.on("response", (res) => {
      res.on("data", (c) => chunks.push(c));
      res.on("end", () => resolve({ status: res.statusCode, body: Buffer.concat(chunks).toString("utf8") }));
      res.on("error", reject);
    });
    req.on("error", reject);
    req.end();
  });
}


async function refreshAccessToken() {
  const cookies = loadCookies();
  if (!cookies.length) {
    throw new Error("Not logged in. Click Login to ChatGPT first.");
  }

  const { status, body } = await netGet(SESSION_URL, browserHeaders(cookies));

  if (status === 401 || status === 403) {
    throw new Error("Session expired. Please login to ChatGPT again.");
  }

  let data;
  try {
    data = JSON.parse(body);
  } catch {
    throw new Error("Unexpected response from ChatGPT session endpoint.");
  }

  if (!data.accessToken) {
    throw new Error("No access token returned. If you see WARNING_BANNER, login first.");
  }

  return data.accessToken;
}

function loginWithBrowserWindow() {
  return new Promise((resolve, reject) => {
    const loginSession = session.fromPartition("persist:chatgpt-login");

    const loginWin = new BrowserWindow({
      width: 1024,
      height: 768,
      title: "Log in to ChatGPT (this window closes automatically when done)",
      icon: ICON_PATH,
      webPreferences: {
        session: loginSession,
        nodeIntegration: false,
        contextIsolation: true
      }
    });

    let captured = false;

    async function tryCaptureOnLoad() {
      if (captured) return;
      const url = loginWin.webContents.getURL();
      if (!url.startsWith("https://chatgpt.com") || url.includes("/auth/")) return;

      captured = true;

      // Poll for the actual session-token cookie — it may arrive after page load
      let cookies = [];
      for (let i = 0; i < 20; i++) {
        await new Promise((r) => setTimeout(r, 500));
        cookies = await loginSession.cookies.get({ url: "https://chatgpt.com" });
        if (cookies.some((c) => c.name.includes("session-token"))) break;
      }

      saveCookies(cookies);
      setTimeout(() => { if (!loginWin.isDestroyed()) loginWin.close(); }, 300);
      resolve(cookies.some((c) => c.name.includes("session-token")));
    }

    loginWin.webContents.on("did-finish-load", () => {
      tryCaptureOnLoad().catch(reject);
    });

    loginWin.on("closed", () => {
      if (!captured) resolve(hasCookies());
    });

    loginWin.loadURL(LOGIN_URL);
  });
}

function peakAmplitudeFromWav(buf) {
  // Parse a 16-bit PCM WAV — return peak |sample| in [0, 32767]
  if (buf.length < 44 || buf.toString("ascii", 0, 4) !== "RIFF") return null;
  let off = 12;
  while (off < buf.length - 8) {
    const id = buf.toString("ascii", off, off + 4);
    const size = buf.readUInt32LE(off + 4);
    if (id === "data") {
      let max = 0;
      for (let i = off + 8; i < Math.min(off + 8 + size, buf.length) - 1; i += 2) {
        const v = Math.abs(buf.readInt16LE(i));
        if (v > max) max = v;
      }
      return max;
    }
    off += 8 + size;
  }
  return null;
}

async function transcribeAudio(audioBuffer, mimeType = "audio/webm") {
  const buf = Buffer.isBuffer(audioBuffer) ? audioBuffer : Buffer.from(audioBuffer);

  // Catch silent captures before we waste a network round-trip
  if (mimeType.includes("wav")) {
    const peak = peakAmplitudeFromWav(buf);
    if (peak !== null && peak < 100) {
      throw new Error(
        "Microphone captured silence. Check Windows mic permissions for desktop apps " +
        "(click 'Open Mic Settings' below) and verify the correct input device is set as default."
      );
    }
  }

  const accessToken = await refreshAccessToken();
  const cookies = loadCookies();

  const ext = mimeType.includes("webm") ? "webm" : mimeType.includes("wav") ? "wav" : "bin";

  const formData = new FormData();
  formData.append("file", new Blob([buf], { type: mimeType }), `recording.${ext}`);
  formData.append("model", "whisper-1");

  const response = await net.fetch(TRANSCRIBE_URL, {
    method: "POST",
    headers: {
      Authorization: `Bearer ${accessToken}`,
      Cookie: buildCookieHeader(cookies),
      "User-Agent": "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Safari/537.36",
      Accept: "*/*",
      "Accept-Language": "en-US,en;q=0.5",
      "oai-language": "en-US",
      "oai-device-id": getDeviceId(),
      "oai-session-id": OAI_SESSION_ID,
      "oai-client-build-number": "5503767",
      "oai-client-version": "prod-afc820ec804df4f6558617d74ea3ba20401b43e1",
      "x-openai-target-path": "/backend-api/transcribe",
      "x-openai-target-route": "/backend-api/transcribe"
    },
    body: formData
  });

  const responseText = await response.text();

  if (!response.ok) {
    throw new Error(`Transcription failed (${response.status}): ${responseText.slice(0, 300)}`);
  }

  let payload;
  try { payload = JSON.parse(responseText); } catch {
    throw new Error("Non-JSON response: " + responseText.slice(0, 200));
  }
  return payload.text || payload.transcript || payload.message || "";
}

async function validateSession() {
  if (!hasCookies()) return false;
  try {
    await refreshAccessToken();
    return true;
  } catch {
    return false;
  }
}

module.exports = {
  hasCookies,
  validateSession,
  loginWithBrowserWindow,
  refreshAccessToken,
  transcribeAudio
};

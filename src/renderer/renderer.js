const statusEl = document.getElementById("status");
const detailEl = document.getElementById("detail");
const loginBtn = document.getElementById("loginButton");
const openLoginPageBtn = document.getElementById("openLoginPageButton");
const saveSessionBtn = document.getElementById("saveSessionButton");
const sessionJsonInput = document.getElementById("sessionJsonInput");
const settingsBtn = document.getElementById("settingsButton");
const timerEl = document.getElementById("timer");
const stopButton = document.getElementById("stopButton");
const recordingOverlay = document.getElementById("recordingOverlay");

let mediaRecorder;
let chunks = [];
let timerHandle;
let seconds = 0;
let maxRecordingSeconds = 120;

function formatSeconds(value) {
  const mins = String(Math.floor(value / 60)).padStart(2, "0");
  const secs = String(value % 60).padStart(2, "0");
  return `${mins}:${secs}`;
}

function setStatus(label, detail = "") {
  statusEl.textContent = label;
  detailEl.textContent = detail;
}

function resetTimer() {
  clearInterval(timerHandle);
  seconds = 0;
  timerEl.textContent = "00:00";
}

function startTimer() {
  clearInterval(timerHandle);
  timerHandle = setInterval(() => {
    seconds += 1;
    timerEl.textContent = formatSeconds(seconds);
    if (seconds >= maxRecordingSeconds) {
      stopRecording().catch((error) => {
        setStatus("Error", error.message || "Auto-stop failed.");
      });
    }
  }, 1000);
}

async function startRecording(payload) {
  maxRecordingSeconds = payload?.maxRecordingSeconds ?? 120;
  resetTimer();

  try {
    const stream = await navigator.mediaDevices.getUserMedia({ audio: true });
    mediaRecorder = new MediaRecorder(stream, { mimeType: "audio/webm" });
    chunks = [];

    mediaRecorder.ondataavailable = (event) => {
      if (event.data && event.data.size > 0) {
        chunks.push(event.data);
      }
    };

    mediaRecorder.start();
    stopButton.disabled = false;
    recordingOverlay.classList.remove("hidden");
    setStatus("Recording", "Speak now. Press Escape or click Stop to finish.");
    startTimer();
  } catch (error) {
    setStatus("Error", error.message || "Microphone unavailable.");
  }
}

async function stopRecording() {
  if (!mediaRecorder || mediaRecorder.state === "inactive") {
    return;
  }

  setStatus("Working", "Finalizing audio...");

  const done = new Promise((resolve) => {
    mediaRecorder.onstop = async () => {
      const blob = new Blob(chunks, { type: mediaRecorder.mimeType || "audio/webm" });
      const bytes = await blob.arrayBuffer();
      resolve({ bytes, mimeType: blob.type || "audio/webm" });
      mediaRecorder.stream.getTracks().forEach((track) => track.stop());
    };
  });

  mediaRecorder.stop();
  stopButton.disabled = true;
  clearInterval(timerHandle);

  const { bytes, mimeType } = await done;
  const result = await window.voiceBridge.finalizeRecording(Array.from(new Uint8Array(bytes)), mimeType);

  if (!result.ok) {
    setStatus("Error", result.error || "Transcription failed.");
    recordingOverlay.classList.add("hidden");
    return;
  }

  setStatus("Ready", "Transcript inserted!");
  recordingOverlay.classList.add("hidden");
}

loginBtn.addEventListener("click", async () => {
  loginBtn.disabled = true;
  setStatus("Working", "Opening /api/auth/session in your browser...");
  const ok = await window.voiceBridge.loginChatGPT();
  loginBtn.disabled = false;
  if (ok) {
    setStatus("Ready", "Auth already available.");
  } else {
    setStatus("Ready", "If only WARNING_BANNER appears, open login page and sign in first.");
  }
});

openLoginPageBtn.addEventListener("click", async () => {
  openLoginPageBtn.disabled = true;
  setStatus("Working", "Opening ChatGPT login page in your browser...");
  await window.voiceBridge.openChatGPTLoginPage();
  openLoginPageBtn.disabled = false;
  setStatus("Ready", "After login, open /api/auth/session and paste full JSON here.");
});

saveSessionBtn.addEventListener("click", async () => {
  const sessionText = sessionJsonInput.value.trim();
  if (!sessionText) {
    setStatus("Error", "Paste full /api/auth/session JSON first.");
    return;
  }

  saveSessionBtn.disabled = true;
  try {
    await window.voiceBridge.saveChatGPTSessionText(sessionText);
    setStatus("Success", "Session data saved. You can now transcribe.");
    sessionJsonInput.value = "";
  } catch (error) {
    setStatus("Error", error?.message || "Failed to parse/save session JSON.");
  } finally {
    saveSessionBtn.disabled = false;
  }
});

settingsBtn.addEventListener("click", () => {
  window.voiceBridge.openSettings();
});

stopButton.addEventListener("click", () => {
  stopRecording().catch((error) => {
    setStatus("Error", error.message || "Stop failed.");
  });
});

window.voiceBridge.onStartRecording((_event, payload) => {
  startRecording(payload).catch((error) => {
    setStatus("Error", error.message || "Failed to start recording.");
  });
});

window.voiceBridge.onStopRecording(() => {
  stopRecording().catch((error) => {
    setStatus("Error", error.message || "Stop failed.");
  });
});

window.voiceBridge.onRecordingStatus((payload) => {
  if (!payload) return;

  if (payload.status === "idle") {
    stopButton.disabled = true;
    resetTimer();
    recordingOverlay.classList.add("hidden");
    setStatus("Ready", "Press Ctrl+Shift+R in any text field to record.");
  }

  if (payload.status === "working") {
    setStatus("Working", payload.detail || "Please wait...");
  }

  if (payload.status === "error") {
    setStatus("Error", payload.detail || "Unknown error.");
    recordingOverlay.classList.add("hidden");
  }

  if (payload.status === "success") {
    setStatus("Success", payload.detail || "Completed.");
    recordingOverlay.classList.add("hidden");
  }
});

setStatus("Ready", "Open /api/auth/session and import session JSON to get started.");

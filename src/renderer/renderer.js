const statusEl = document.getElementById("status");
const detailEl = document.getElementById("detail");
const loginBtn = document.getElementById("loginButton");
const settingsBtn = document.getElementById("settingsButton");
const authBadge = document.getElementById("authBadge");
const micSettingsBtn = document.getElementById("micSettingsBtn");

function setStatus(label, detail = "") {
  statusEl.textContent = label;
  detailEl.textContent = detail;
  const isMicError = /microphone|mic permission|silent/i.test(detail);
  micSettingsBtn.style.display = isMicError ? "block" : "none";
}

micSettingsBtn.addEventListener("click", () => {
  window.voiceBridge.openMicSettings();
});

function setAuthBadge(loggedIn) {
  authBadge.textContent = loggedIn ? "signed in, we're set" : "not yet. let's fix that.";
  authBadge.className = "auth-badge " + (loggedIn ? "auth-ok" : "auth-none");
}

async function checkAuth() {
  const ok = await window.voiceBridge.getAuthStatus();
  setAuthBadge(ok);
}

loginBtn.addEventListener("click", async () => {
  loginBtn.disabled = true;
  setStatus("just a moment", "opening the ChatGPT sign-in window. it'll close itself.");
  const ok = await window.voiceBridge.loginChatGPT();
  loginBtn.disabled = false;
  setAuthBadge(ok);
  if (ok) {
    refreshHotkeyDisplay();
  } else {
    setStatus("ready", "the sign-in window closed early. try again whenever.");
  }
});

settingsBtn.addEventListener("click", () => {
  window.voiceBridge.openSettings();
});

window.voiceBridge.onRecordingStatus((payload) => {
  if (!payload) return;

  if (payload.status === "idle") {
    refreshHotkeyDisplay();
  } else if (payload.status === "recording") {
    setStatus("listening", "say what's on your mind. Escape stops it.");
  } else if (payload.status === "working") {
    setStatus("a moment", payload.detail || "thinking…");
  } else if (payload.status === "error") {
    setStatus("hmm", payload.detail || "something went sideways");
  } else if (payload.status === "success") {
    setStatus("done", payload.detail || "your words landed where the cursor was");
  }
});

// ── Microphone picker + live level meter ────────────────────────────────────
const micSelect    = document.getElementById("micSelect");
const testMicBtn   = document.getElementById("testMicBtn");
const micLevelWrap = document.getElementById("micLevelWrap");
const micLevelBar  = document.getElementById("micLevelBar");
const micLevelText = document.getElementById("micLevelText");

let testCtx = null;

async function loadMicList() {
  try {
    // Need permission to see device labels. A throwaway getUserMedia call grants it.
    const probe = await navigator.mediaDevices.getUserMedia({ audio: true });
    probe.getTracks().forEach((t) => t.stop());

    const devices = await navigator.mediaDevices.enumerateDevices();
    const inputs = devices.filter((d) => d.kind === "audioinput");
    const prefs = await window.voiceBridge.getPreferences();

    micSelect.innerHTML = "";
    const defaultOpt = document.createElement("option");
    defaultOpt.value = "default";
    defaultOpt.textContent = "System default";
    micSelect.appendChild(defaultOpt);

    for (const d of inputs) {
      const opt = document.createElement("option");
      opt.value = d.deviceId;
      opt.textContent = d.label || `Microphone (${d.deviceId.slice(0, 8)})`;
      micSelect.appendChild(opt);
    }

    micSelect.value = prefs.inputDeviceId || "default";
  } catch (err) {
    micSelect.innerHTML = `<option>Error: ${err.message}</option>`;
  }
}

micSelect.addEventListener("change", async () => {
  await window.voiceBridge.savePreferences({ inputDeviceId: micSelect.value });
  if (testCtx) await stopMicTest(); // restart the test against the new device
  startMicTest();
});

async function startMicTest() {
  if (testCtx) return;
  micLevelWrap.style.display = "block";
  micLevelText.textContent = "listening, say something to confirm.";

  try {
    const stream = await navigator.mediaDevices.getUserMedia({
      audio: {
        deviceId: micSelect.value === "default" ? undefined : { exact: micSelect.value },
        echoCancellation: false,
        noiseSuppression: false,
        autoGainControl: true
      }
    });

    const ctx = new AudioContext();
    const src = ctx.createMediaStreamSource(stream);
    const analyser = ctx.createAnalyser();
    analyser.fftSize = 1024;
    src.connect(analyser);

    const data = new Uint8Array(analyser.fftSize);
    let peakSeen = 0;
    let raf;

    const tick = () => {
      analyser.getByteTimeDomainData(data);
      let peak = 0;
      for (let i = 0; i < data.length; i++) {
        const v = Math.abs(data[i] - 128);
        if (v > peak) peak = v;
      }
      if (peak > peakSeen) peakSeen = peak;
      const pct = Math.min(100, (peak / 128) * 100 * 4); // visual amplification
      micLevelBar.style.width = pct.toFixed(0) + "%";
      raf = requestAnimationFrame(tick);
    };
    tick();

    testCtx = { ctx, stream, raf, peakRef: () => peakSeen };
    testMicBtn.textContent = "Stop the test";
  } catch (err) {
    micLevelText.textContent = "couldn't open the mic. " + err.message;
  }
}

async function stopMicTest() {
  if (!testCtx) return;
  cancelAnimationFrame(testCtx.raf);
  testCtx.stream.getTracks().forEach((t) => t.stop());
  try { await testCtx.ctx.close(); } catch {}
  const peak = testCtx.peakRef();
  testCtx = null;
  testMicBtn.textContent = "Test it. speak softly, the bar should move.";
  micLevelBar.style.width = "0%";
  if (peak < 5) {
    micLevelText.textContent = "✕ silence. try a different device above.";
  } else {
    micLevelText.textContent = `✓ peak ${peak}/128. this one works, saved.`;
  }
}

testMicBtn.addEventListener("click", () => {
  if (testCtx) stopMicTest();
  else startMicTest();
});

async function refreshHotkeyDisplay() {
  const prefs = await window.voiceBridge.getPreferences();
  const pretty = (s) => (s || "").replace(/CommandOrControl/g, "Ctrl");
  const startEl = document.getElementById("startKbd");
  const stopEl  = document.getElementById("stopKbd");
  if (startEl && prefs.startHotkey) startEl.textContent = pretty(prefs.startHotkey);
  if (stopEl  && prefs.stopHotkey)  stopEl.textContent  = pretty(prefs.stopHotkey);
  setStatus("ready", `click into any text field, then press ${pretty(prefs.startHotkey)}`);
}

loadMicList();
checkAuth();
refreshHotkeyDisplay();
// Re-read prefs whenever the prefs window saves something (poll on focus)
window.addEventListener("focus", refreshHotkeyDisplay);

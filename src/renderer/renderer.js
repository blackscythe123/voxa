/* ════════════════════════════════════════════════════════════════════════
   Voxa — Hub router (production)
   ────────────────────────────────────────────────────────────────────────
   Vanilla ES, no framework, no build step. Runs directly in the Electron
   renderer. Clones a <template id="tpl-<name>"> into #view, toggles the
   active sidebar item, and runs mount<Name>().

   EVERY window.voiceBridge access is guarded so the file never throws when
   the bridge is absent (static agent-browser preview must still render the
   shell + navigate between empty views). Navigation is pure DOM.

   Contract = src/renderer/index.html (element IDs + <template> structure),
   styles/hub.css + styles/history.css (state classes: .active, .open,
   .selected, .switch.on, .hotkey-input.recording, .toast.show, …).
   ════════════════════════════════════════════════════════════════════════ */
(() => {
  "use strict";

  // ── Bridge guard ────────────────────────────────────────────────────────
  // bridge() returns window.voiceBridge or null. call(name, ...args) invokes
  // a bridge method, swallows any throw, and returns the result (or a promise
  // resolving to fallback). Never lets a missing/broken bridge break the UI.
  const bridge = () => (typeof window !== "undefined" ? window.voiceBridge : null) || null;

  async function call(method, ...args) {
    const b = bridge();
    if (!b || typeof b[method] !== "function") return undefined;
    try {
      return await b[method](...args);
    } catch (err) {
      console.warn(`[voxa] voiceBridge.${method} failed:`, err);
      return undefined;
    }
  }

  // subscribe(method, cb) — for the on*() event bridges. Returns an
  // unsubscribe fn (the bridge's own, if it returns one) or a no-op.
  function subscribe(method, cb) {
    const b = bridge();
    if (!b || typeof b[method] !== "function") return () => {};
    try {
      const off = b[method](cb);
      return typeof off === "function" ? off : () => {};
    } catch (err) {
      console.warn(`[voxa] voiceBridge.${method} subscribe failed:`, err);
      return () => {};
    }
  }

  // ── Tiny DOM helpers ────────────────────────────────────────────────────
  const $ = (sel, root = document) => root.querySelector(sel);
  const $$ = (sel, root = document) => Array.from(root.querySelectorAll(sel));
  const byId = (id, root = document) => (root.getElementById ? root.getElementById(id) : root.querySelector("#" + id));

  function setText(el, text) {
    if (el) el.textContent = text == null ? "" : String(text);
  }

  // ── Formatting helpers ──────────────────────────────────────────────────
  function escapeHtml(s) {
    return String(s == null ? "" : s)
      .replace(/&/g, "&amp;")
      .replace(/</g, "&lt;")
      .replace(/>/g, "&gt;")
      .replace(/"/g, "&quot;")
      .replace(/'/g, "&#39;");
  }

  function fmtDuration(ms) {
    const total = Math.max(0, Math.round((Number(ms) || 0) / 1000));
    const m = Math.floor(total / 60);
    const s = total % 60;
    return `${m}:${String(s).padStart(2, "0")}`;
  }

  // MediaRecorder WebM clips have no Duration in their EBML header, so the
  // <audio> element reports duration=Infinity (a dead seek bar / "0:00").
  // Force a seek to the end: the browser then reads the real duration, which
  // we reset back to 0. This makes the native player's timeline work.
  function fixInfiniteDuration(audio) {
    if (!audio) return;
    const onMeta = () => {
      audio.removeEventListener("loadedmetadata", onMeta);
      if (audio.duration !== Infinity) return;
      const onUpdate = () => {
        audio.removeEventListener("timeupdate", onUpdate);
        if (isFinite(audio.duration)) {
          try { audio.currentTime = 0; } catch (_) {}
        }
      };
      audio.addEventListener("timeupdate", onUpdate);
      try { audio.currentTime = 1e101; } catch (_) {}
    };
    audio.addEventListener("loadedmetadata", onMeta);
  }

  function relativeTime(ms) {
    const t = Number(ms);
    if (!t || Number.isNaN(t)) return "";
    const now = Date.now();
    const diff = now - t;
    if (diff < 0) return "just now";
    const sec = Math.floor(diff / 1000);
    if (sec < 45) return "just now";
    const min = Math.floor(sec / 60);
    if (min < 60) return `${min} min ago`;
    // Same calendar day → clock time (e.g. "10:58 am")
    const d = new Date(t);
    const today = new Date(now);
    const sameDay =
      d.getFullYear() === today.getFullYear() &&
      d.getMonth() === today.getMonth() &&
      d.getDate() === today.getDate();
    if (sameDay) {
      return d
        .toLocaleTimeString([], { hour: "numeric", minute: "2-digit" })
        .toLowerCase();
    }
    const day = 24 * 60 * 60 * 1000;
    const days = Math.floor(diff / day);
    if (days === 1) return "yesterday";
    if (days < 7) return `${days} days ago`;
    return d.toLocaleDateString([], { month: "short", day: "numeric" });
  }

  // Day-bucket: Today / Yesterday / This Week / Older — lowercased headers.
  function dayBucket(ms) {
    const t = Number(ms);
    const now = new Date();
    const startOfToday = new Date(now.getFullYear(), now.getMonth(), now.getDate()).getTime();
    const day = 24 * 60 * 60 * 1000;
    if (t >= startOfToday) return { key: "today", label: "today", order: 0 };
    if (t >= startOfToday - day) return { key: "yesterday", label: "yesterday", order: 1 };
    if (t >= startOfToday - 6 * day) return { key: "week", label: "this week", order: 2 };
    return { key: "older", label: "older", order: 3 };
  }

  // Friendly error copy keyed by error_kind.
  function errorCopy(kind) {
    switch (kind) {
      case "network":
        return "couldn't reach ChatGPT — saved, try again";
      case "auth":
        return "the ChatGPT session expired — sign in again, then retry";
      case "server":
        return "ChatGPT hiccuped on its end — your audio is safe, give it another go";
      case "silence":
        return "the take came back silent — check the mic, then re-record";
      case "timeout":
        return "it took too long and timed out — saved, try again";
      default:
        return "something went sideways — your audio is saved, try again";
    }
  }

  // status → dot class
  function dotClassFor(status) {
    switch (status) {
      case "ok":
        return "dot-on";
      case "failed":
        return "dot-off";
      case "empty":
        return "dot-empty";
      case "pending":
        return "dot-pending";
      default:
        return "dot-empty";
    }
  }

  // ── Toasts ──────────────────────────────────────────────────────────────
  // Slide-up bottom-right, auto-dismiss. Built on .toast-stack/.toast from
  // history.css. kind ∈ '' | 'ok' | 'error' | 'warn'.
  let toastStack = null;
  function ensureToastStack() {
    if (toastStack && document.body.contains(toastStack)) return toastStack;
    toastStack = document.querySelector(".toast-stack");
    if (!toastStack) {
      toastStack = document.createElement("div");
      toastStack.className = "toast-stack";
      document.body.appendChild(toastStack);
    }
    return toastStack;
  }

  function toast(msg, kind = "", title = "") {
    const stack = ensureToastStack();
    const el = document.createElement("div");
    el.className = "toast" + (kind ? " " + kind : "");

    const icon = document.createElementNS("http://www.w3.org/2000/svg", "svg");
    icon.setAttribute("viewBox", "0 0 24 24");
    icon.setAttribute("fill", "none");
    icon.setAttribute("stroke-width", "1.5");
    icon.setAttribute("stroke-linecap", "round");
    icon.setAttribute("stroke-linejoin", "round");
    const path = document.createElementNS("http://www.w3.org/2000/svg", "path");
    // check for ok, alert-triangle for error/warn, dot otherwise
    if (kind === "error" || kind === "warn") {
      path.setAttribute("d", "M12 9v4M12 16.5h.01M10.3 3.9 2.5 18a2 2 0 0 0 1.7 3h15.6a2 2 0 0 0 1.7-3L13.7 3.9a2 2 0 0 0-3.4 0Z");
    } else {
      path.setAttribute("d", "M5 12.5 10 17.5 19.5 7");
    }
    icon.appendChild(path);
    el.appendChild(icon);

    const body = document.createElement("div");
    if (title) {
      const t = document.createElement("div");
      t.className = "toast-title";
      t.textContent = title;
      body.appendChild(t);
    }
    const m = document.createElement("div");
    m.className = "toast-msg";
    m.textContent = msg;
    body.appendChild(m);
    el.appendChild(body);

    stack.appendChild(el);
    // force reflow then add .show for the transition
    void el.offsetWidth;
    el.classList.add("show");

    const remove = () => {
      el.classList.remove("show");
      window.setTimeout(() => {
        if (el.parentNode) el.parentNode.removeChild(el);
      }, 320);
    };
    window.setTimeout(remove, 2400);
    return el;
  }

  // ════════════════════════════════════════════════════════════════════════
  // Router core
  // ════════════════════════════════════════════════════════════════════════
  const ROUTES = ["home", "history", "microphone", "shortcuts", "settings"];

  // Per-route teardown — mount fns may register cleanup (unsubscribe, stop
  // mic test, etc.) which runs on route-away.
  let activeCleanups = [];
  function registerCleanup(fn) {
    if (typeof fn === "function") activeCleanups.push(fn);
  }
  function runCleanups() {
    const fns = activeCleanups;
    activeCleanups = [];
    for (const fn of fns) {
      try {
        fn();
      } catch (err) {
        console.warn("[voxa] cleanup failed:", err);
      }
    }
  }

  let currentRoute = null;

  function route(name) {
    if (!ROUTES.includes(name)) name = "home";

    // tear down whatever the previous view set up
    runCleanups();
    closeDrawer(true); // ensure drawer is gone when switching views

    const view = byId("view");
    const tpl = byId("tpl-" + name);
    if (!view || !tpl || !("content" in tpl)) {
      console.warn(`[voxa] cannot mount route '${name}' — missing #view or #tpl-${name}`);
      return;
    }

    view.innerHTML = "";
    view.appendChild(tpl.content.cloneNode(true));
    view.scrollTop = 0;

    // active nav item
    $$(".nav-item").forEach((el) => {
      el.classList.toggle("active", el.getAttribute("data-view") === name);
    });

    currentRoute = name;

    const mounts = {
      home: mountHome,
      history: mountHistory,
      microphone: mountMicrophone,
      shortcuts: mountShortcuts,
      settings: mountSettings,
    };
    const fn = mounts[name];
    if (fn) {
      try {
        fn(view);
      } catch (err) {
        console.error(`[voxa] mount${name} threw:`, err);
      }
    }
  }

  // ════════════════════════════════════════════════════════════════════════
  // Auth badge / pip (shared between Home + sidebar + Settings)
  // ════════════════════════════════════════════════════════════════════════
  function normalizeAuth(res) {
    // getAuthStatus may return a bool (legacy) or { signedIn, ageDays?, email? }
    if (typeof res === "boolean") return { signedIn: res };
    if (res && typeof res === "object") return res;
    return { signedIn: false };
  }

  function applyAuthPip(auth) {
    // Update the sidebar profile's status dot + fallback label.
    const dotEl = document.querySelector("#profileAvatar .dot");
    if (dotEl) dotEl.className = "dot " + (auth.signedIn ? "dot-on" : "dot-off");
    const nameEl = byId("profileName");
    const emailEl = byId("profileEmail");
    if (!auth.signedIn) {
      if (nameEl) setText(nameEl, "Signed out");
      if (emailEl) setText(emailEl, "");
    } else if (nameEl && /checking/i.test(nameEl.textContent || "")) {
      setText(nameEl, "Signed in");
    }
  }

  function applyProfileIdentity(account) {
    if (!account) return;
    const nameEl = byId("profileName");
    const emailEl = byId("profileEmail");
    const avatar = byId("profileAvatar");
    if (nameEl) setText(nameEl, account.name || account.email || "Signed in");
    if (emailEl) setText(emailEl, account.email || "");
    if (avatar && account.image) {
      const img = document.createElement("img");
      img.alt = "";
      img.referrerPolicy = "no-referrer";
      img.onload = () => { avatar.innerHTML = ""; avatar.appendChild(img); };
      img.onerror = () => { /* keep the dot fallback */ };
      img.src = account.image;
    }
  }

  // Sidebar profile (bottom-left): identity + Sign out / Re-authenticate.
  async function mountProfile() {
    const auth = normalizeAuth(await call("getAuthStatus"));
    applyAuthPip(auth);
    if (auth.signedIn) {
      const account = await call("getAccountInfo");
      applyProfileIdentity(account);
    }
    const reauth = byId("reauthBtnSidebar");
    if (reauth && !reauth.dataset.wired) {
      reauth.dataset.wired = "1";
      reauth.addEventListener("click", async () => {
        reauth.disabled = true;
        const res = await call("loginChatGPT");
        reauth.disabled = false;
        await mountProfile();
        const ok = !!(res && (res.signedIn || res.ok));
        toast(ok ? "re-authenticated" : "sign-in didn't complete", ok ? "ok" : "error");
      });
    }
    const signOut = byId("signOutBtnSidebar");
    if (signOut && !signOut.dataset.wired) {
      signOut.dataset.wired = "1";
      signOut.addEventListener("click", async () => {
        if (!window.confirm("Sign out of ChatGPT? You'll need to sign in again to transcribe.")) return;
        await call("signOut");
        await mountProfile();
        toast("signed out", "ok");
      });
    }
  }

  function applyAuthBadge(auth) {
    const badge = byId("authBadge");
    if (!badge) return;
    const span = badge.querySelector("span") || badge;
    badge.className = "auth-badge " + (auth.signedIn ? "auth-ok" : "auth-none");
    let msg;
    if (auth.signedIn) {
      msg = auth.ageDays != null ? `signed in — cookies ${auth.ageDays}d old, we're set` : "signed in, we're set";
    } else {
      msg = "not yet. let's fix that.";
    }
    setText(span, msg);
    const loginBtn = byId("loginButton");
    if (loginBtn) loginBtn.style.display = auth.signedIn ? "none" : "";
  }

  async function refreshAuth() {
    const auth = normalizeAuth(await call("getAuthStatus"));
    applyAuthPip(auth);
    applyAuthBadge(auth);
    if (auth.signedIn) call("getAccountInfo").then(applyProfileIdentity);
    return auth;
  }

  // ════════════════════════════════════════════════════════════════════════
  // HOME
  // ════════════════════════════════════════════════════════════════════════
  async function mountHome(view) {
    // stats
    const stats = (await call("historyStats", {})) || {};
    setText(byId("statTodayCount", view), stats.countToday != null ? stats.countToday : "0");
    setText(byId("statTodayWords", view), stats.wordsToday != null ? stats.wordsToday : "0");
    setText(byId("statWeekWords", view), stats.wordsThisWeek != null ? stats.wordsThisWeek : "0");

    // live status panel
    const statusEl = byId("status", view);
    const detailEl = byId("detail", view);
    const liveDot = view.querySelector(".live-status .dot");

    function setStatus(label, detail = "", dotClass = "dot-pending") {
      setText(statusEl, label);
      setText(detailEl, detail);
      if (liveDot) liveDot.className = "dot " + dotClass;
    }

    const off = subscribe("onRecordingStatus", (payload) => {
      if (!payload) return;
      switch (payload.status) {
        case "idle":
          setStatus("ready", payload.detail || "click into any text field, then press your start key", "dot-on");
          break;
        case "recording":
          setStatus("listening", payload.detail || "say what's on your mind. your stop key ends it.", "dot-pending");
          break;
        case "working":
          setStatus("a moment", payload.detail || "transcribing…", "dot-pending");
          break;
        case "success":
          setStatus("done", payload.detail || "your words landed where the cursor was", "dot-on");
          break;
        case "error":
          setStatus("hmm", payload.detail || "something went sideways", "dot-off");
          break;
        default:
          if (payload.detail) setStatus(payload.status || "…", payload.detail);
      }
    });
    registerCleanup(off);

    // auth
    await refreshAuth();

    // login button
    const loginBtn = byId("loginButton", view);
    if (loginBtn) {
      loginBtn.addEventListener("click", async () => {
        loginBtn.disabled = true;
        setStatus("just a moment", "opening the ChatGPT sign-in window. it'll close itself.", "dot-pending");
        const res = await call("loginChatGPT");
        const auth = normalizeAuth(res && res.signedIn != null ? res : { signedIn: !!(res && res.ok) });
        loginBtn.disabled = false;
        applyAuthPip(auth);
        applyAuthBadge(auth);
        if (auth.signedIn) {
          toast("signed in to ChatGPT", "ok");
          setStatus("ready", "click into any text field, then press your start key", "dot-on");
        } else {
          setStatus("ready", "the sign-in window closed early. try again whenever.", "dot-off");
        }
      });
    }

    // hotkey kbd labels
    const prefs = (await call("getPreferences")) || {};
    const pretty = (s) => String(s || "").replace(/CommandOrControl/g, "Ctrl");
    if (prefs.startHotkey) setText(byId("startKbd", view), pretty(prefs.startHotkey));
    if (prefs.stopHotkey) setText(byId("stopKbd", view), pretty(prefs.stopHotkey));
    setStatus(
      "ready",
      prefs.startHotkey ? `click into any text field, then press ${pretty(prefs.startHotkey)}` : "ready when you are",
      "dot-on"
    );

    // open history link
    const link = byId("openHistoryLink", view);
    if (link) {
      link.style.cursor = "pointer";
      link.addEventListener("click", (e) => {
        e.preventDefault();
        route("history");
      });
    }
  }

  // ════════════════════════════════════════════════════════════════════════
  // HISTORY
  // ════════════════════════════════════════════════════════════════════════
  let historyState = null; // { view, search, prefs, debounceTimer, loaded }
  const HISTORY_PAGE_SIZE = 50;

  async function mountHistory(view) {
    historyState = { view, search: "", prefs: {}, debounceTimer: null, loaded: HISTORY_PAGE_SIZE };
    historyState.prefs = (await call("getPreferences")) || {};

    // search box w/ 200ms debounce
    const searchInput = byId("historySearch", view);
    if (searchInput) {
      searchInput.addEventListener("input", () => {
        window.clearTimeout(historyState.debounceTimer);
        historyState.debounceTimer = window.setTimeout(() => {
          historyState.search = searchInput.value.trim();
          historyState.loaded = HISTORY_PAGE_SIZE; // reset paging on a new search
          renderHistoryList();
        }, 200);
      });
    }

    // refresh on any history change while History is mounted
    const off = subscribe("onHistoryChanged", () => {
      // re-fetch + re-render; cheap enough for a desktop history list
      refreshHistory();
    });
    registerCleanup(off);

    // dismiss any open context menu on scroll / outside click while mounted
    const onDocClick = (e) => {
      const menu = document.querySelector(".row-context-menu");
      if (menu && !menu.contains(e.target) && !e.target.closest(".row-menu")) closeContextMenu();
    };
    document.addEventListener("click", onDocClick, true);
    registerCleanup(() => document.removeEventListener("click", onDocClick, true));

    await refreshHistory();
  }

  async function refreshHistory() {
    await renderFailedBanner();
    await renderHistoryList();
  }

  async function renderFailedBanner() {
    if (!historyState) return;
    const { view, prefs } = historyState;
    const section = byId("lastFailedSection", view);
    const rowsHost = byId("lastFailedRows", view);
    const countEl = byId("lastFailedCount", view);
    if (!section || !rowsHost) return;

    if (!prefs.showLastFailedSection) {
      section.hidden = true;
      return;
    }

    const cap = Number(prefs.lastFailedBannerCap) || 5;
    const failures = (await call("historyGetRecentFailures", { limit: cap })) || [];

    if (!Array.isArray(failures) || failures.length === 0) {
      section.hidden = true;
      rowsHost.innerHTML = "";
      return;
    }

    section.hidden = false;
    rowsHost.innerHTML = "";
    setText(countEl, `${failures.length} recent`);

    const tpl = byId("tpl-failed-row");
    for (const row of failures) {
      let node;
      if (tpl && "content" in tpl) {
        node = tpl.content.firstElementChild.cloneNode(true);
      } else {
        node = document.createElement("div");
        node.className = "failed-row";
      }
      node.setAttribute("data-id", row.id);
      const dot = node.querySelector(".row-dot, .dot");
      if (dot) dot.className = "dot " + dotClassFor(row.status) + " row-dot";
      setText(node.querySelector(".row-transcript, .preview"), previewText(row));
      setText(node.querySelector(".row-meta, .meta"), failedMetaLine(row));
      const retryBtn = node.querySelector(".failed-retry-btn, .retry-btn");
      if (retryBtn) {
        const disabled = !canRetry(row);
        retryBtn.disabled = disabled;
        if (disabled) retryBtn.title = "audio no longer on disk — can't retry";
        retryBtn.addEventListener("click", (e) => {
          e.stopPropagation();
          handleRetry(row.id, retryBtn);
        });
      }
      rowsHost.appendChild(node);
    }
  }

  async function renderHistoryList() {
    if (!historyState) return;
    const { view, search } = historyState;
    const groupsHost = byId("historyGroups", view);
    const emptyEl = byId("historyEmpty", view);
    if (!groupsHost) return;

    const loaded = historyState.loaded || HISTORY_PAGE_SIZE;
    const res =
      (await call("historyList", { limit: loaded, offset: 0, search: search || undefined })) || {};
    const rows = Array.isArray(res.rows) ? res.rows : [];
    const total = res.total != null ? res.total : rows.length;

    groupsHost.innerHTML = "";

    if (total === 0 || rows.length === 0) {
      if (emptyEl) emptyEl.hidden = false;
      return;
    }
    if (emptyEl) emptyEl.hidden = true;

    // group by day bucket, preserving row order within each bucket
    const buckets = new Map();
    for (const row of rows) {
      const b = dayBucket(row.created_at);
      if (!buckets.has(b.key)) buckets.set(b.key, { label: b.label, order: b.order, rows: [] });
      buckets.get(b.key).rows.push(row);
    }
    const ordered = Array.from(buckets.values()).sort((a, b) => a.order - b.order);

    const groupTpl = byId("tpl-history-group");
    const rowTpl = byId("tpl-history-row");

    for (const group of ordered) {
      let groupNode, header, rowsContainer;
      if (groupTpl && "content" in groupTpl) {
        groupNode = groupTpl.content.firstElementChild.cloneNode(true);
        header = groupNode.querySelector(".group-header, .group-head");
        rowsContainer = groupNode.querySelector(".group-rows");
      } else {
        groupNode = document.createElement("div");
        groupNode.className = "history-group";
        header = document.createElement("h3");
        header.className = "group-head";
        rowsContainer = document.createElement("div");
        groupNode.append(header, rowsContainer);
      }
      setText(header, group.label);

      for (const row of group.rows) {
        rowsContainer.appendChild(buildHistoryRow(row, rowTpl));
      }
      groupsHost.appendChild(groupNode);
    }

    // Windowed paging: if more rows exist than we've loaded, offer "Load more".
    if (total > rows.length) {
      const remaining = total - rows.length;
      const more = document.createElement("button");
      more.type = "button";
      more.className = "load-more-btn";
      more.textContent = `Load more (${remaining} older)`;
      more.addEventListener("click", () => {
        historyState.loaded = (historyState.loaded || HISTORY_PAGE_SIZE) + HISTORY_PAGE_SIZE;
        renderHistoryList();
      });
      groupsHost.appendChild(more);
    }
  }

  function buildHistoryRow(row, rowTpl) {
    let node;
    if (rowTpl && "content" in rowTpl) {
      node = rowTpl.content.firstElementChild.cloneNode(true);
    } else {
      node = document.createElement("div");
      node.className = "history-row hist-row";
      node.innerHTML = '<span class="dot row-dot"></span><div class="rtext"><div class="preview row-transcript"></div><div class="meta row-meta"></div></div><button class="row-menu row-menu-btn"></button>';
    }
    node.setAttribute("data-id", row.id);

    const dot = node.querySelector(".row-dot, .dot");
    if (dot) dot.className = "dot " + dotClassFor(row.status) + " row-dot";

    const preview = node.querySelector(".row-transcript, .preview");
    if (preview) {
      const text = previewText(row);
      setText(preview, text);
      if (!row.transcript) preview.classList.add("empty");
    }
    setText(node.querySelector(".row-meta, .meta"), rowMetaLine(row));

    // row click → drawer (ignore clicks on the menu button)
    node.addEventListener("click", (e) => {
      if (e.target.closest(".row-menu-btn, .row-menu")) return;
      openDrawer(row.id);
    });

    const menuBtn = node.querySelector(".row-menu-btn, .row-menu");
    if (menuBtn) {
      menuBtn.addEventListener("click", (e) => {
        e.stopPropagation();
        openContextMenu(row, menuBtn);
      });
    }
    return node;
  }

  function previewText(row) {
    if (row.transcript && row.transcript.trim()) return row.transcript.trim();
    switch (row.status) {
      case "failed":
        return "— transcription failed";
      case "empty":
        return "— no speech detected";
      case "pending":
        return "— transcribing…";
      default:
        return "—";
    }
  }

  function rowMetaLine(row) {
    const parts = [relativeTime(row.created_at)];
    if (row.duration_ms) parts.push(fmtDuration(row.duration_ms));
    const app = row.source_app_label || row.source_app;
    if (app) parts.push(app);
    return parts.filter(Boolean).join(" · ");
  }

  function failedMetaLine(row) {
    const parts = [relativeTime(row.created_at)];
    const app = row.source_app_label || row.source_app;
    if (app) parts.push(app);
    if (row.error_kind) parts.push(row.error_kind);
    return parts.filter(Boolean).join(" · ");
  }

  function canRetry(row) {
    // Only real failures with audio still on disk. Never offer Retry for an
    // empty/"no speech" row — re-sending silence is pointless.
    return row.status === "failed" && !!row.audio_filename && !row.audio_purged_at;
  }

  // ── Row context menu ─────────────────────────────────────────────────────
  let contextMenuEl = null;
  function closeContextMenu() {
    if (contextMenuEl && contextMenuEl.parentNode) contextMenuEl.parentNode.removeChild(contextMenuEl);
    contextMenuEl = null;
  }

  function menuItem(label, iconPath, handler, danger = false) {
    const btn = document.createElement("button");
    btn.type = "button";
    btn.className = "menu-item" + (danger ? " danger" : "");
    const svg = document.createElementNS("http://www.w3.org/2000/svg", "svg");
    svg.setAttribute("viewBox", "0 0 24 24");
    svg.setAttribute("fill", "none");
    svg.setAttribute("stroke-width", "1.5");
    svg.setAttribute("stroke-linecap", "round");
    svg.setAttribute("stroke-linejoin", "round");
    const p = document.createElementNS("http://www.w3.org/2000/svg", "path");
    p.setAttribute("d", iconPath);
    svg.appendChild(p);
    btn.appendChild(svg);
    btn.appendChild(document.createTextNode(label));
    btn.addEventListener("click", (e) => {
      e.stopPropagation();
      closeContextMenu();
      handler();
    });
    return btn;
  }

  function openContextMenu(row, anchorBtn) {
    closeContextMenu();
    const menu = document.createElement("div");
    menu.className = "row-context-menu";

    menu.appendChild(menuItem("Copy", "M9 9h11v11H9zM5 15V5a2 2 0 0 1 2-2h10", () => handleCopy(row.id)));
    menu.appendChild(menuItem("Paste into focused app", "M9 10 4 15l5 5M4 15h11a5 5 0 0 0 5-5V5", () => handleReinsert(row.id)));
    const retryItem = menuItem("Retry", "M21 12a9 9 0 1 1-2.6-6.4M21 3v4.5h-4.5", () => handleRetry(row.id));
    if (!canRetry(row)) {
      retryItem.disabled = true;
      retryItem.style.opacity = "0.4";
      retryItem.style.pointerEvents = "none";
    }
    menu.appendChild(retryItem);
    const revealItem = menuItem("Reveal audio", "M3 7a2 2 0 0 1 2-2h3l2 2.5h9a2 2 0 0 1 2 2V18a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2Z", () => handleReveal(row.id));
    if (!row.audio_filename || row.audio_purged_at) {
      revealItem.disabled = true;
      revealItem.style.opacity = "0.4";
      revealItem.style.pointerEvents = "none";
    }
    menu.appendChild(revealItem);
    menu.appendChild(menuItem("Delete", "M4 6h16M9 6V4h6v2M6 6l1 14h10l1-14", () => handleDelete(row.id), true));

    document.body.appendChild(menu);
    contextMenuEl = menu;

    // position below-left of the button, clamped to viewport
    const r = anchorBtn.getBoundingClientRect();
    const mw = menu.offsetWidth || 168;
    const mh = menu.offsetHeight || 200;
    let left = r.right - mw;
    let top = r.bottom + 6;
    if (left < 8) left = 8;
    if (top + mh > window.innerHeight - 8) top = r.top - mh - 6;
    menu.style.left = `${Math.round(left)}px`;
    menu.style.top = `${Math.round(top)}px`;
  }

  // ════════════════════════════════════════════════════════════════════════
  // Drawer
  // ════════════════════════════════════════════════════════════════════════
  let drawerKeyHandler = null;
  let drawerOutsideHandler = null;

  function markRowSelected(id) {
    $$(".history-row, .hist-row").forEach((el) => {
      el.classList.toggle("selected", el.getAttribute("data-id") === String(id));
    });
  }
  function clearRowSelected() {
    $$(".history-row, .hist-row").forEach((el) => el.classList.remove("selected"));
  }

  async function openDrawer(id) {
    const drawer = byId("drawer");
    if (!drawer) return;
    const row = await call("historyGet", id);
    if (!row) {
      toast("couldn't load that entry", "error");
      return;
    }
    // Track the row currently shown so async audio fetches can drop stale results.
    drawer.dataset.id = String(row.id);

    const tpl = byId("tpl-drawer-detail");
    drawer.innerHTML = "";
    if (tpl && "content" in tpl) {
      drawer.appendChild(tpl.content.cloneNode(true));
    }

    // badge
    const badge = byId("drawerBadge", drawer);
    if (badge) {
      const isFailed = row.status === "failed";
      badge.className = "status-badge " + (row.status === "ok" ? "ok" : isFailed ? "failed" : "");
      const bdot = badge.querySelector(".dot");
      if (bdot) bdot.className = "dot " + dotClassFor(row.status);
      // append status label text
      const labelMap = { ok: "Saved", failed: "Failed", empty: "No speech", pending: "Pending" };
      Array.from(badge.childNodes).forEach((n) => {
        if (n.nodeType === Node.TEXT_NODE) n.remove();
      });
      badge.appendChild(document.createTextNode(labelMap[row.status] || row.status || ""));
    }

    // transcript
    setText(byId("drawerTranscript", drawer), row.transcript || previewText(row));

    // audio — play it if still on disk; otherwise say so plainly (don't show a
    // dead player). audio_purged_at set = cleared to save space; null filename
    // = never captured (e.g. a crash mid-recording).
    const audio = byId("drawerAudio", drawer);
    const audioBlock = audio ? audio.closest(".audio-block") : null;
    const goneNote = byId("drawerAudioGone", drawer);
    const audioAvailable = row.audio_filename && !row.audio_purged_at;
    if (audio && audioBlock) {
      audio.removeAttribute("src");
      audioBlock.hidden = false;
      const showGone = (msg) => {
        audio.hidden = true;
        if (goneNote) { goneNote.hidden = false; setText(goneNote, msg); }
      };
      const showPlayer = () => {
        audio.hidden = false;
        if (goneNote) goneNote.hidden = true;
      };
      if (audioAvailable) {
        showPlayer();
        call("historyAudioData", row.id).then((dataUrl) => {
          if (drawer.dataset.id && drawer.dataset.id !== String(row.id)) return;
          if (!dataUrl) { showGone("audio no longer available"); return; }
          audio.src = dataUrl;
          audio.load();
          fixInfiniteDuration(audio);
        });
      } else if (!row.audio_filename) {
        showGone("no audio was captured for this recording");
      } else {
        showGone("audio no longer available — it was cleared to save space");
      }
    }

    // meta grid
    fillMetaGrid(byId("drawerMeta", drawer), row);

    // error box — red "what went wrong" for real failures; a calm neutral note
    // for "no speech" (an empty/silent recording is not an error).
    const errorBox = byId("drawerError", drawer);
    if (errorBox) {
      const etitle = errorBox.querySelector(".etitle");
      const emsg = errorBox.querySelector(".emsg");
      if (row.status === "failed") {
        errorBox.hidden = false;
        errorBox.classList.remove("info");
        if (etitle) setText(etitle, "WHAT WENT WRONG");
        setText(emsg, row.error_message || errorCopy(row.error_kind));
      } else if (row.status === "empty") {
        errorBox.hidden = false;
        errorBox.classList.add("info");
        if (etitle) setText(etitle, "NO SPEECH DETECTED");
        setText(emsg, "There was no audio to transcribe — check that your microphone isn't muted or set to the wrong device.");
      } else {
        errorBox.hidden = true;
        errorBox.classList.remove("info");
      }
    }

    // wire action buttons
    wireDrawerActions(drawer, row, audioAvailable);

    // reveal
    drawer.hidden = false;
    void drawer.offsetWidth; // reflow so the transition fires
    drawer.classList.add("open");
    markRowSelected(id);

    // close affordances
    const backBtn = byId("drawerBack", drawer);
    const closeBtn = byId("drawerClose", drawer);
    if (backBtn) backBtn.addEventListener("click", () => closeDrawer());
    if (closeBtn) closeBtn.addEventListener("click", () => closeDrawer());

    drawerKeyHandler = (e) => {
      if (e.key === "Escape") closeDrawer();
    };
    document.addEventListener("keydown", drawerKeyHandler);

    drawerOutsideHandler = (e) => {
      if (!drawer.contains(e.target) && !e.target.closest(".history-row, .hist-row, .row-context-menu")) {
        closeDrawer();
      }
    };
    // defer so the opening click doesn't immediately close it
    window.setTimeout(() => document.addEventListener("click", drawerOutsideHandler, true), 0);
  }

  function wireDrawerActions(drawer, row, audioAvailable) {
    const copyBtn = byId("drawerCopy", drawer);
    const reinsertBtn = byId("drawerReinsert", drawer);
    const retryBtn = byId("drawerRetry", drawer);
    const revealBtn = byId("drawerReveal", drawer);
    const deleteBtn = byId("drawerDelete", drawer);

    const hasTranscript = !!(row.transcript && row.transcript.trim());
    if (copyBtn) {
      copyBtn.disabled = !hasTranscript;
      copyBtn.addEventListener("click", () => handleCopy(row.id));
    }
    if (reinsertBtn) {
      reinsertBtn.disabled = !hasTranscript;
      reinsertBtn.addEventListener("click", () => handleReinsert(row.id));
    }
    if (retryBtn) {
      retryBtn.disabled = !canRetry(row);
      if (retryBtn.disabled) retryBtn.title = "audio no longer on disk — can't retry";
      retryBtn.addEventListener("click", () => handleRetry(row.id, retryBtn));
    }
    if (revealBtn) {
      revealBtn.disabled = !audioAvailable;
      revealBtn.addEventListener("click", () => handleReveal(row.id));
    }
    if (deleteBtn) {
      deleteBtn.addEventListener("click", () => handleDelete(row.id));
    }
  }

  function fillMetaGrid(grid, row) {
    if (!grid) return;
    grid.innerHTML = "";
    const cells = [
      ["model", row.model_id || "—"],
      ["status", row.status || "—"],
      ["duration", row.duration_ms ? fmtDuration(row.duration_ms) : "—"],
      ["words", row.word_count != null ? row.word_count : "—"],
      ["retries", row.retry_count != null ? row.retry_count : "0"],
      ["latency", row.latency_ms != null ? `${row.latency_ms} ms` : "—"],
    ];
    for (const [k, v] of cells) {
      const cell = document.createElement("div");
      cell.className = "cell";
      const kEl = document.createElement("div");
      kEl.className = "k";
      kEl.textContent = k;
      const vEl = document.createElement("div");
      vEl.className = "v";
      vEl.textContent = String(v);
      if (k === "status" && row.status === "failed") vEl.style.color = "var(--signal-off)";
      cell.append(kEl, vEl);
      grid.appendChild(cell);
    }
  }

  function closeDrawer(immediate = false) {
    const drawer = byId("drawer");
    if (!drawer) return;
    if (drawerKeyHandler) {
      document.removeEventListener("keydown", drawerKeyHandler);
      drawerKeyHandler = null;
    }
    if (drawerOutsideHandler) {
      document.removeEventListener("click", drawerOutsideHandler, true);
      drawerOutsideHandler = null;
    }
    clearRowSelected();
    closeContextMenu();

    const wasOpen = drawer.classList.contains("open");
    drawer.classList.remove("open");

    const finish = () => {
      drawer.hidden = true;
      drawer.innerHTML = "";
    };
    if (immediate || !wasOpen) {
      finish();
    } else {
      // re-add [hidden] after the slide-out transition (280ms in hub.css)
      window.setTimeout(finish, 300);
    }
  }

  // ════════════════════════════════════════════════════════════════════════
  // History actions
  // ════════════════════════════════════════════════════════════════════════
  async function handleRetry(id, btnEl) {
    if (btnEl) {
      btnEl.disabled = true;
      btnEl.dataset.label = btnEl.textContent;
      // keep any icon; just swap the trailing text node if present
      const textNode = Array.from(btnEl.childNodes).find((n) => n.nodeType === Node.TEXT_NODE && n.textContent.trim());
      if (textNode) textNode.textContent = " retrying…";
      else btnEl.textContent = "retrying…";
    }
    const res = await call("historyRetry", id);
    if (res && res.ok) {
      toast("transcribed · pasted in", "ok");
      await refreshHistoryAfterAction(id);
    } else {
      const kind = res && res.errorKind;
      toast(errorCopy(kind), "error");
      if (btnEl) {
        btnEl.disabled = false;
        const textNode = Array.from(btnEl.childNodes).find((n) => n.nodeType === Node.TEXT_NODE);
        if (textNode) textNode.textContent = btnEl.dataset.label || " Retry";
      }
    }
  }

  async function handleCopy(id) {
    const res = await call("historyCopy", id);
    if (res === undefined || (res && res.ok !== false)) toast("copied", "ok");
    else toast("couldn't copy", "error");
  }

  async function handleReinsert(id) {
    const res = await call("historyReinsert", id);
    if (res === undefined || (res && res.ok !== false)) toast("pasted into focused app", "ok");
    else toast("couldn't paste — copy it instead", "error");
  }

  async function handleReveal(id) {
    const res = await call("historyRevealAudio", id);
    if (res && res.ok === false) toast("the audio file is no longer on disk", "warn");
  }

  async function handleDelete(id) {
    if (!window.confirm("Delete this dictation for good? The audio goes too.")) return;
    const res = await call("historyDelete", id);
    if (res === undefined || (res && res.ok !== false)) {
      toast("deleted", "ok");
      closeDrawer();
      if (currentRoute === "history") await refreshHistory();
    } else {
      toast("couldn't delete", "error");
    }
  }

  // After a mutating action: refresh list + banner, and if the drawer is open
  // for this id, reload it to reflect the new state.
  async function refreshHistoryAfterAction(id) {
    if (currentRoute === "history") await refreshHistory();
    const drawer = byId("drawer");
    if (drawer && !drawer.hidden && drawer.classList.contains("open")) {
      await openDrawer(id);
    }
  }

  // ════════════════════════════════════════════════════════════════════════
  // MICROPHONE  (ported from the prior renderer.js)
  // ════════════════════════════════════════════════════════════════════════
  function mountMicrophone(view) {
    const micSelect = byId("micSelect", view);
    const testMicBtn = byId("testMicBtn", view);
    const micLevelWrap = byId("micLevelWrap", view);
    const micLevelBar = byId("micLevelBar", view);
    const micLevelText = byId("micLevelText", view);
    const micSettingsBtn = byId("micSettingsBtn", view);

    let testCtx = null;

    async function loadMicList() {
      if (!micSelect) return;
      try {
        const probe = await navigator.mediaDevices.getUserMedia({ audio: true });
        probe.getTracks().forEach((t) => t.stop());

        const devices = await navigator.mediaDevices.enumerateDevices();
        const inputs = devices.filter((d) => d.kind === "audioinput");
        const prefs = (await call("getPreferences")) || {};

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
        micSelect.innerHTML = "";
        const opt = document.createElement("option");
        opt.textContent = "Couldn't list devices: " + err.message;
        micSelect.appendChild(opt);
      }
    }

    async function startMicTest() {
      if (testCtx || !micSelect) return;
      if (micLevelWrap) micLevelWrap.hidden = false;
      if (micLevelText) micLevelText.textContent = "listening — say something to confirm.";
      try {
        const stream = await navigator.mediaDevices.getUserMedia({
          audio: {
            deviceId: micSelect.value === "default" ? undefined : { exact: micSelect.value },
            echoCancellation: false,
            noiseSuppression: false,
            autoGainControl: true,
          },
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
          const pct = Math.min(100, (peak / 128) * 100 * 4);
          if (micLevelBar) micLevelBar.style.width = pct.toFixed(0) + "%";
          raf = requestAnimationFrame(tick);
        };
        tick();
        testCtx = { ctx, stream, raf, peakRef: () => peakSeen };
        setTestBtnLabel("Stop the test");
      } catch (err) {
        if (micLevelText) micLevelText.textContent = "couldn't open the mic. " + err.message;
      }
    }

    async function stopMicTest() {
      if (!testCtx) return;
      cancelAnimationFrame(testCtx.raf);
      testCtx.stream.getTracks().forEach((t) => t.stop());
      try {
        await testCtx.ctx.close();
      } catch (_) {}
      const peak = testCtx.peakRef();
      testCtx = null;
      setTestBtnLabel("Test it");
      if (micLevelBar) micLevelBar.style.width = "0%";
      if (micLevelText) {
        micLevelText.textContent =
          peak < 5 ? "silence — try a different device above." : `peak ${peak}/128 — this one works, saved.`;
      }
    }

    function setTestBtnLabel(label) {
      if (!testMicBtn) return;
      const textNode = Array.from(testMicBtn.childNodes).find((n) => n.nodeType === Node.TEXT_NODE && n.textContent.trim());
      if (textNode) textNode.textContent = " " + label;
      else testMicBtn.appendChild(document.createTextNode(" " + label));
    }

    if (micSelect) {
      micSelect.addEventListener("change", async () => {
        await call("savePreferences", { inputDeviceId: micSelect.value });
        if (testCtx) {
          await stopMicTest();
          startMicTest();
        }
      });
    }
    if (testMicBtn) {
      testMicBtn.addEventListener("click", () => {
        if (testCtx) stopMicTest();
        else startMicTest();
      });
    }
    if (micSettingsBtn) {
      micSettingsBtn.style.cursor = "pointer";
      micSettingsBtn.addEventListener("click", (e) => {
        e.preventDefault();
        call("openMicSettings");
      });
    }

    // tear down the live test when navigating away
    registerCleanup(() => {
      if (testCtx) {
        try {
          cancelAnimationFrame(testCtx.raf);
          testCtx.stream.getTracks().forEach((t) => t.stop());
          testCtx.ctx.close();
        } catch (_) {}
        testCtx = null;
      }
    });

    loadMicList();
  }

  // ════════════════════════════════════════════════════════════════════════
  // SHORTCUTS  (keychord capture)
  // ════════════════════════════════════════════════════════════════════════
  function mountShortcuts(view) {
    const startInput = byId("startHotkeyInput", view);
    const stopInput = byId("stopHotkeyInput", view);
    const pretty = (s) => String(s || "").replace(/CommandOrControl/g, "Ctrl");

    const fields = {
      start: { el: startInput, hotkey: "F9" },
      stop: { el: stopInput, hotkey: "Esc" },
    };

    function renderField(target) {
      const f = fields[target];
      if (!f || !f.el) return;
      let kbd = f.el.querySelector("kbd");
      if (!kbd) {
        kbd = document.createElement("kbd");
        f.el.prepend(kbd);
      }
      kbd.textContent = pretty(f.hotkey) || "—";
    }

    async function loadPrefs() {
      const prefs = (await call("getPreferences")) || {};
      if (prefs.startHotkey) fields.start.hotkey = prefs.startHotkey;
      if (prefs.stopHotkey) fields.stop.hotkey = prefs.stopHotkey;
      renderField("start");
      renderField("stop");
    }

    async function persist() {
      await call("savePreferences", { startHotkey: fields.start.hotkey, stopHotkey: fields.stop.hotkey });
    }

    // Build an Electron-style accelerator string from a keydown event.
    function chordFromEvent(e) {
      const mods = [];
      if (e.ctrlKey || e.metaKey) mods.push("CommandOrControl");
      if (e.altKey) mods.push("Alt");
      if (e.shiftKey) mods.push("Shift");

      let key = e.key;
      if (["Control", "Alt", "Shift", "Meta", "OS"].includes(key)) return null; // modifier alone
      if (key === " ") key = "Space";
      else if (key === "Escape") key = "Esc";
      else if (/^[a-z]$/.test(key)) key = key.toUpperCase();
      else if (/^F\d{1,2}$/.test(key)) {
        /* function keys keep as-is */
      } else if (key.length === 1) {
        key = key.toUpperCase();
      }
      return mods.concat(key).join("+");
    }

    function beginCapture(target) {
      const f = fields[target];
      if (!f || !f.el) return;
      // stop any other capturing field
      Object.values(fields).forEach((other) => other.el && other.el.classList.remove("recording"));
      f.el.classList.add("recording");
      f.el.focus();

      const onKey = (e) => {
        e.preventDefault();
        e.stopPropagation();
        const chord = chordFromEvent(e);
        if (!chord) return; // wait for a non-modifier key
        f.hotkey = chord;
        renderField(target);
        endCapture(target);
        persist();
        toast(`${target === "start" ? "start" : "stop"} key set to ${pretty(chord)}`, "ok");
      };
      const onBlur = () => endCapture(target);
      f._onKey = onKey;
      f._onBlur = onBlur;
      f.el.addEventListener("keydown", onKey);
      f.el.addEventListener("blur", onBlur);
    }

    function endCapture(target) {
      const f = fields[target];
      if (!f || !f.el) return;
      f.el.classList.remove("recording");
      if (f._onKey) f.el.removeEventListener("keydown", f._onKey);
      if (f._onBlur) f.el.removeEventListener("blur", f._onBlur);
      f._onKey = f._onBlur = null;
    }

    [["start", startInput], ["stop", stopInput]].forEach(([target, el]) => {
      if (!el) return;
      el.addEventListener("click", (e) => {
        if (e.target.closest(".clear-btn")) return;
        beginCapture(target);
      });
      el.addEventListener("focus", () => {
        if (!el.classList.contains("recording")) beginCapture(target);
      });
    });

    // clear buttons
    $$(".clear-btn", view).forEach((btn) => {
      btn.addEventListener("click", (e) => {
        e.stopPropagation();
        const target = btn.getAttribute("data-target");
        const f = fields[target];
        if (!f) return;
        f.hotkey = "";
        renderField(target);
        persist();
      });
    });

    // quick-pick chips
    $$(".quick-pick", view).forEach((chip) => {
      chip.addEventListener("click", () => {
        const target = chip.getAttribute("data-target");
        const key = chip.getAttribute("data-key");
        const f = fields[target];
        if (!f) return;
        // normalize chip key to accelerator form (Ctrl → CommandOrControl)
        f.hotkey = key.replace(/\bCtrl\b/g, "CommandOrControl");
        renderField(target);
        persist();
        toast(`${target === "start" ? "start" : "stop"} key set to ${pretty(f.hotkey)}`, "ok");
      });
    });

    registerCleanup(() => {
      endCapture("start");
      endCapture("stop");
    });

    loadPrefs();
  }

  // ════════════════════════════════════════════════════════════════════════
  // SETTINGS
  // ════════════════════════════════════════════════════════════════════════
  async function mountSettings(view) {
    const prefs = (await call("getPreferences")) || {};
    const autoLaunch = await call("getAutoLaunch");

    // ── anchor chips → scroll to section ──
    $$(".anchor-chip", view).forEach((chip) => {
      chip.style.cursor = "pointer";
      chip.addEventListener("click", () => {
        const sec = view.querySelector(`.settings-section[data-section="${chip.getAttribute("data-anchor")}"]`);
        if (sec) sec.scrollIntoView({ behavior: "smooth", block: "start" });
      });
    });

    // ── toggle (switch) helper ──
    function wireSwitch(id, initial, onChange) {
      const sw = byId(id, view);
      if (!sw) return;
      const set = (on) => {
        sw.classList.toggle("on", !!on);
        sw.setAttribute("aria-checked", on ? "true" : "false");
      };
      set(initial);
      const toggle = () => {
        const next = !sw.classList.contains("on");
        set(next);
        onChange(next);
      };
      sw.addEventListener("click", toggle);
      sw.addEventListener("keydown", (e) => {
        if (e.key === " " || e.key === "Enter") {
          e.preventDefault();
          toggle();
        }
      });
    }

    // Startup
    wireSwitch("autoLaunch", !!autoLaunch, async (on) => {
      await call("setAutoLaunch", on);
      toast(on ? "Voxa will launch on boot" : "auto-launch off", "ok");
    });

    // ── select helper ──
    function wireSelect(id, value, key) {
      const el = byId(id, view);
      if (!el) return;
      if (value != null) el.value = value;
      el.addEventListener("change", async () => {
        await call("savePreferences", { [key]: el.value });
      });
    }
    wireSelect("overlayPosition", prefs.overlayPosition, "overlayPosition");

    // ── slider helper (with live *Val label) ──
    function wireSlider(id, valId, key, value, fmt, toStore) {
      const el = byId(id, view);
      const valEl = byId(valId, view);
      if (!el) return;
      if (value != null) el.value = value;
      const paint = () => {
        if (valEl) valEl.textContent = fmt ? fmt(el.value) : el.value;
      };
      paint();
      el.addEventListener("input", paint);
      el.addEventListener("change", async () => {
        const stored = toStore ? toStore(Number(el.value)) : Number(el.value);
        await call("savePreferences", { [key]: stored });
      });
    }
    // Max recording: a minutes slider (1–10) whose final notch (11) means
    // "no limit" — stored as 0, which tells the overlay never to auto-stop.
    const maxRecToPos = (sec) => sec === 0 ? 11 : Math.min(10, Math.max(1, Math.round((Number(sec) || 0) / 60)));
    wireSlider(
      "maxRecordingSeconds", "maxRecordingSecondsVal", "maxRecordingSeconds",
      maxRecToPos(prefs.maxRecordingSeconds),
      (v) => Number(v) >= 11 ? "no limit" : `${v} min`,
      (v) => v >= 11 ? 0 : v * 60
    );
    wireSlider("successAudioRetention", "successAudioRetentionVal", "successAudioRetentionHours", prefs.successAudioRetentionHours,
      (v) => Number(v) === 0 ? "keep" : (Number(v) >= 24 ? `${Math.round(v / 24)}d` : `${v}h`));
    wireSlider("failedAudioRetention", "failedAudioRetentionVal", "failedAudioRetentionDays", prefs.failedAudioRetentionDays, (v) => `${v}d`);

    // privacy mode + history cap toggles
    wireSwitch("privacyMode", !!prefs.privacyMode, async (on) => {
      await call("savePreferences", { privacyMode: on });
    });
    // historyMaxEntries is a numeric cap; the toggle means "cap on/off".
    // On → a sensible default cap; off → 0 (keep everything).
    wireSwitch("historyMaxEntries", Number(prefs.historyMaxEntries) > 0, async (on) => {
      await call("savePreferences", { historyMaxEntries: on ? Number(prefs.historyMaxEntries) || 500 : 0 });
    });

    // ── Account ──
    const auth = normalizeAuth(await call("getAuthStatus"));
    const sessionLine = byId("accountSession", view);
    if (sessionLine) {
      const span = sessionLine.querySelector("span") || sessionLine;
      setText(
        span,
        auth.signedIn
          ? auth.email
            ? `signed in as ${auth.email}`
            : "signed in"
          : "signed out — re-authenticate below"
      );
    }
    applyAuthPip(auth);

    const reauthBtn = byId("reauthBtn", view);
    if (reauthBtn) {
      reauthBtn.addEventListener("click", async () => {
        reauthBtn.disabled = true;
        const res = await call("loginChatGPT");
        reauthBtn.disabled = false;
        const next = normalizeAuth(res && res.signedIn != null ? res : { signedIn: !!(res && res.ok) });
        applyAuthPip(next);
        if (sessionLine) {
          const span = sessionLine.querySelector("span") || sessionLine;
          setText(span, next.signedIn ? (next.email ? `signed in as ${next.email}` : "signed in") : "still signed out");
        }
        toast(next.signedIn ? "re-authenticated" : "sign-in didn't complete", next.signedIn ? "ok" : "error");
      });
    }

    const signOutBtn = byId("signOutBtn", view);
    if (signOutBtn) {
      signOutBtn.addEventListener("click", async () => {
        if (!window.confirm("Sign out of ChatGPT? You'll need to sign in again to transcribe.")) return;
        // No dedicated signOut bridge method exists; surface intent + refresh.
        const did = await call("signOut");
        await refreshAuth();
        toast(did === undefined ? "sign-out isn't available yet" : "signed out", did === undefined ? "warn" : "ok");
      });
    }

    // ── About ──
    const openLogs = byId("openLogsBtn", view);
    if (openLogs) {
      openLogs.style.cursor = "pointer";
      openLogs.addEventListener("click", (e) => {
        e.preventDefault();
        call("openLogsFolder");
      });
    }
    const openData = byId("openDataBtn", view);
    if (openData) {
      openData.style.cursor = "pointer";
      openData.addEventListener("click", (e) => {
        e.preventDefault();
        call("openDataFolder");
      });
    }

    // app version
    const verEl = byId("appVersion", view);
    if (verEl) {
      const ver = await call("getAppVersion");
      setText(verEl, ver || "");
    }
  }

  // ════════════════════════════════════════════════════════════════════════
  // Boot
  // ════════════════════════════════════════════════════════════════════════
  function wireSidebar() {
    $$(".nav-item").forEach((item) => {
      item.addEventListener("click", () => {
        const view = item.getAttribute("data-view");
        if (view) route(view);
      });
    });
  }

  function boot() {
    wireSidebar();

    // tray "jump to section" navigation
    subscribe("onNavigate", (target) => {
      if (typeof target === "string" && ROUTES.includes(target)) route(target);
    });

    // a finished dictation while sitting on Home/History → refresh stats/list
    subscribe("onDictationComplete", () => {
      if (currentRoute === "history") refreshHistory();
      if (currentRoute === "home") {
        call("historyStats", {}).then((stats) => {
          if (!stats) return;
          const v = byId("view");
          if (!v) return;
          setText(byId("statTodayCount", v), stats.countToday);
          setText(byId("statTodayWords", v), stats.wordsToday);
          setText(byId("statWeekWords", v), stats.wordsThisWeek);
        });
      }
    });

    // sidebar profile (identity + sign out / re-auth), persistent across routes
    mountProfile();

    route("home");
  }

  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", boot);
  } else {
    boot();
  }
})();

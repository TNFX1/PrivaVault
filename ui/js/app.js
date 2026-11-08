// PrivaVault UI logic.
// Rules: never use innerHTML with data (entry names come from untrusted vaults); never handle
// output paths (Rust opens every save/folder dialog); keep passwords only in input fields and
// clear them as soon as they are no longer needed.
"use strict";

const TAURI = window.__TAURI__;
const invoke = TAURI.core.invoke;
const listen = TAURI.event.listen;

const $ = (id) => document.getElementById(id);
const state = {
  settings: null,
  info: null,
  lang: "en",
  view: "encrypt",
  selection: [],
  picked: null,
  vault: null,
  busy: false,
  lastActivity: Date.now(),
  updateUrl: null,
};

// ---------------------------------------------------------------- i18n & formatting
function t(key, vars) {
  const dict = window.I18N[state.lang] || window.I18N.en;
  let s = dict[key] ?? window.I18N.en[key] ?? key;
  if (vars) for (const [k, v] of Object.entries(vars)) s = s.split(`{${k}}`).join(String(v));
  return s;
}

function resolveLang() {
  const pref = state.settings?.language || "auto";
  if (pref !== "auto") return pref;
  return (navigator.language || "en").toLowerCase().startsWith("tr") ? "tr" : "en";
}

function applyI18n() {
  state.lang = resolveLang();
  document.documentElement.lang = state.lang;
  document.querySelectorAll("[data-i18n]").forEach((el) => (el.textContent = t(el.dataset.i18n)));
  document.querySelectorAll("[data-i18n-placeholder]").forEach((el) => (el.placeholder = t(el.dataset.i18nPlaceholder)));
  document.querySelectorAll("[data-i18n-aria]").forEach((el) => el.setAttribute("aria-label", t(el.dataset.i18nAria)));
  renderSelection();
  renderVault();
  renderEncResult();
  renderVaultResult();
  renderActivity();
  renderRailInfo();
  renderNet();
  renderAbout();
  updateMeter();
}

function fmtBytes(n) {
  if (!n) return state.lang === "tr" ? "0 bayt" : "0 bytes";
  const units = ["B", "KB", "MB", "GB", "TB"];
  const i = Math.min(units.length - 1, Math.floor(Math.log(n) / Math.log(1024)));
  const v = n / Math.pow(1024, i);
  const num = new Intl.NumberFormat(state.lang, { maximumFractionDigits: i === 0 ? 0 : v < 10 ? 1 : 0 }).format(v);
  return `${num} ${units[i]}`;
}

function fmtDuration(sec) {
  sec = Math.max(0, Math.round(sec));
  const m = Math.floor(sec / 60), s = sec % 60;
  if (state.lang === "tr") return m ? `${m} dk ${s} sn` : `${s} sn`;
  return m ? `${m} min ${s} s` : `${s} s`;
}

function fmtDate(unix) {
  if (!unix) return "";
  return new Intl.DateTimeFormat(state.lang, { dateStyle: "medium", timeStyle: "short" }).format(new Date(unix * 1000));
}

// ---------------------------------------------------------------- DOM helpers
function h(tag, attrs = {}, ...children) {
  const el = document.createElement(tag);
  for (const [k, v] of Object.entries(attrs)) {
    if (k === "class") el.className = v;
    else if (k === "text") el.textContent = v;
    // CSS custom properties via CSSOM: allowed by the CSP (a style="" attribute would be blocked).
    else if (k === "vars") for (const [name, val] of Object.entries(v)) el.style.setProperty(name, String(val));
    else if (k.startsWith("on")) el.addEventListener(k.slice(2), v);
    else el.setAttribute(k, v);
  }
  for (const c of children) if (c != null) el.append(c);
  return el;
}

function icon(kind) {
  const ns = "http://www.w3.org/2000/svg";
  const svg = document.createElementNS(ns, "svg");
  svg.setAttribute("viewBox", "0 0 24 24");
  svg.setAttribute("class", "icon");
  svg.setAttribute("aria-hidden", "true");
  const p = document.createElementNS(ns, "path");
  p.setAttribute("fill", "none");
  p.setAttribute("stroke", "currentColor");
  p.setAttribute("stroke-width", "1.7");
  p.setAttribute("stroke-linejoin", "round");
  p.setAttribute("d", kind === "dir"
    ? "M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z"
    : "M6 3h8l4 4v14H6zM14 3v4h4");
  svg.append(p);
  return svg;
}

function showMsg(el, text, ok = false) {
  el.textContent = text;
  el.classList.toggle("ok", ok);
  el.hidden = !text;
}

function errText(e) {
  const code = e && typeof e === "object" && e.code ? e.code : "generic";
  const key = `err.${code}`;
  const msg = window.I18N.en[key] ? t(key) : t("err.generic");
  if (e && e.detail) console.warn(`[${code}]`, e.detail); // technical detail; never contains secrets
  return msg;
}

// ---------------------------------------------------------------- theme & nav
const darkQuery = window.matchMedia("(prefers-color-scheme: dark)");
function applyTheme() {
  const pref = state.settings?.theme || "system";
  // On Linux the desktop preference comes from the XDG portal via Rust (systemDark);
  // elsewhere, or when unknown, the WebView's media query is right.
  const systemDark = typeof state.systemDark === "boolean" ? state.systemDark : darkQuery.matches;
  const dark = pref === "dark" || (pref === "system" && systemDark);
  document.documentElement.dataset.theme = dark ? "dark" : "light";
}
darkQuery.addEventListener("change", applyTheme);

function showView(view) {
  const changed = state.view !== view;
  state.view = view;
  for (const v of ["encrypt", "open", "settings"]) {
    const el = $(`view-${v}`);
    el.hidden = v !== view;
    if (v === view && changed) {
      el.classList.remove("entering");
      void el.offsetWidth; // restart the animation
      el.classList.add("entering");
    }
  }
  document.querySelectorAll(".nav-item").forEach((b) => {
    if (b.dataset.view === view) b.setAttribute("aria-current", "page");
    else b.removeAttribute("aria-current");
  });
  moveIndicator();
}

/** Slide the accent bar to the active item of the main navigation. */
function moveIndicator() {
  const bar = $("navIndicator");
  const active = document.querySelector(".nav .nav-item[aria-current]");
  if (!active) { bar.style.opacity = "0"; return; }
  bar.style.opacity = "1";
  bar.style.height = `${active.offsetHeight - 12}px`;
  bar.style.transform = `translateY(${active.offsetTop + 6}px)`;
}

/** Flatpak: the store updates the app; no update check and no network permission. */
const storeManaged = () => !!state.info?.environment?.store_managed;

function renderNet() {
  const on = !!state.settings?.check_updates && !storeManaged();
  $("netStatus").classList.toggle("online", on);
  $("netText").textContent = on ? t("net.online") : t("net.offline");
}

// ---------------------------------------------------------------- busy overlay & progress
let busyStart = 0;
function setBusy(on) {
  state.busy = on;
  $("busy").hidden = !on;
  if (on) {
    busyStart = Date.now();
    setProgress({ phase: "starting", done: 0, total: 0 });
    $("btnCancel").focus();
  }
}

function setProgress(p) {
  const pct = p.total > 0 ? Math.min(100, (p.done / p.total) * 100) : 0;
  $("busyRing").style.strokeDasharray = `${pct} ${100 - pct}`;
  $("busyPct").textContent = p.total > 0 ? `${Math.floor(pct)}%` : "";
  $("busyPhase").textContent = t(`phase.${p.phase}`);
  if (p.total > 0 && p.done > 0 && (p.phase === "encrypting" || p.phase === "decrypting" || p.phase === "verifying")) {
    const secs = Math.max(0.1, (Date.now() - busyStart) / 1000);
    const speed = p.done / secs;
    $("busyDetail").textContent = t("busy.detail", {
      done: fmtBytes(p.done), total: fmtBytes(p.total), speed: fmtBytes(speed), eta: fmtDuration((p.total - p.done) / speed),
    });
  } else {
    $("busyDetail").textContent = "";
  }
}

async function runOp(fn) {
  setBusy(true);
  try {
    return await fn();
  } finally {
    setBusy(false);
  }
}

function notifyIfAway(title, body) {
  if (document.hidden || !document.hasFocus()) invoke("notify", { title, body }).catch(() => {});
}

// ---------------------------------------------------------------- password strength
const { strength, newer } = window.PVUtil;

function updateMeter() {
  const pw = $("encPw").value;
  const lvl = strength(pw);
  $("pwMeterBar").parentElement.dataset.level = lvl;
  $("pwMeterBar").style.width = `${lvl * 25}%`;
  $("pwMeterText").textContent = pw ? t(["pw.empty", "pw.weak", "pw.fair", "pw.good", "pw.strong"][lvl]) : "";
  const pw2 = $("encPw2").value;
  $("pwMatch").textContent = pw2 ? t(pw === pw2 ? "pw.match" : "pw.nomatch") : "";
}

// ---------------------------------------------------------------- encrypt
function addToSelection(items) {
  state.justAdded = new Set();
  for (const it of items) {
    if (!state.selection.some((s) => s.path === it.path)) {
      state.selection.push(it);
      state.justAdded.add(it.path);
    }
  }
  $("encResult").hidden = true;
  $("encForm").hidden = false;
  showMsg($("encMsg"), "");
  renderSelection();
}

function renderSelection() {
  const sel = state.selection;
  $("selection").hidden = sel.length === 0;
  $("encDrop").classList.toggle("has", sel.length > 0);
  const total = sel.reduce((a, s) => a + s.size, 0);
  $("selSummary").textContent = sel.length === 1 ? t("enc.selOne", { size: fmtBytes(total) }) : t("enc.selFiles", { n: sel.length, size: fmtBytes(total) });
  const list = $("selList");
  let fresh = 0;
  list.replaceChildren(...sel.map((s, i) => h("li", state.justAdded?.has(s.path) ? { class: "new", vars: { "--i": fresh++ } } : {},
    icon(s.is_dir ? "dir" : "file"),
    h("span", { class: "name", title: s.path, text: s.name }),
    h("span", { class: "size", text: s.is_dir ? `${t("enc.folderInfo", { n: s.files })} · ${fmtBytes(s.size)}` : fmtBytes(s.size) }),
    h("button", { class: "remove", type: "button", "aria-label": t("enc.remove", { name: s.name }), text: "×",
      onclick: () => { state.selection.splice(i, 1); renderSelection(); } }),
  )));
  state.justAdded = null;
}

function suggestedName() {
  if (state.selection.length === 1) return state.selection[0].name.replace(/\.[^.]+$/, "") || "vault";
  return `vault-${new Date().toISOString().slice(0, 10)}`;
}

async function startEncrypt(ev) {
  ev.preventDefault();
  const msg = $("encMsg");
  const pw = $("encPw").value, pw2 = $("encPw2").value;
  if (!state.selection.length) return showMsg(msg, t("err.empty_input"));
  if (!pw) return showMsg(msg, t("err.no_password"));
  if (pw.length < 8) { shake($("encPwForm")); return showMsg(msg, t("err.weak")); }
  if (pw !== pw2) { shake($("encPwForm")); return showMsg(msg, t("err.mismatch")); }
  showMsg(msg, "");
  try {
    const res = await runOp(() => invoke("encrypt", { paths: state.selection.map((s) => s.path), password: pw, suggestedName: suggestedName() }));
    if (!res) return; // dialog dismissed
    $("encPw").value = $("encPw2").value = "";
    updateMeter();
    state.selection = [];
    renderSelection();
    $("encForm").hidden = true;
    $("encResult").hidden = false;
    state.encResult = res;
    renderEncResult();
    $("encReveal").focus();
    logActivity("locked", "act.locked", { name: res.output.split(/[\\/]/).pop() });
    notifyIfAway("PrivaVault", t("notify.encDone"));
  } catch (e) {
    showMsg(msg, errText(e));
  }
}

// Results are re-rendered on language change, so keep the data, not the sentence.
function renderEncResult() {
  const res = state.encResult;
  if (!res) return;
  $("encResultText").textContent = t("enc.doneText", { files: res.files, size: fmtBytes(res.bytes) });
  $("encResultPath").textContent = res.output;
  $("encSkipped").hidden = res.skipped.length === 0;
  $("encSkipped").textContent = t("enc.skipped", { n: res.skipped.length });
}

function setVaultResult(render) {
  state.vaultResult = render;
  renderVaultResult();
}

function renderVaultResult() {
  if (!state.vaultResult) return;
  $("vaultResultText").textContent = state.vaultResult();
  $("vaultResult").hidden = false;
}

// ---------------------------------------------------------------- session activity (memory only)
const ACT_ICONS = {
  locked: "M7 10V7a5 5 0 0 1 10 0v3M4.5 10h15v11h-15z",
  unlocked: "M7 10V7a5 5 0 0 1 9.6-1.9M4.5 10h15v11h-15z",
  extracted: "M12 4v11m0 0l-4-4m4 4l4-4M5 20h14",
  closed: "M7 10V7a5 5 0 0 1 10 0v3M4.5 10h15v11h-15z",
};

/** Remember what happened in this session. Never written to disk; gone when the app quits. */
function logActivity(kind, key, vars) {
  state.activity = [{ kind, key, vars, at: Date.now() }, ...(state.activity || [])].slice(0, 6);
  state.activityFresh = true;
  renderActivity();
}

function renderActivity() {
  const list = state.activity || [];
  $("activityEmpty").hidden = list.length > 0;
  const rtf = new Intl.RelativeTimeFormat(state.lang, { numeric: "auto" });
  const ago = (ms) => {
    const s = Math.round((ms - Date.now()) / 1000);
    if (s > -45) return t("act.now");
    if (s > -3600) return rtf.format(Math.round(s / 60), "minute");
    return rtf.format(Math.round(s / 3600), "hour");
  };
  const ns = "http://www.w3.org/2000/svg";
  $("activityList").replaceChildren(...list.map((a, i) => {
    const svg = document.createElementNS(ns, "svg");
    svg.setAttribute("viewBox", "0 0 24 24");
    svg.setAttribute("aria-hidden", "true");
    const p = document.createElementNS(ns, "path");
    p.setAttribute("d", ACT_ICONS[a.kind] || ACT_ICONS.locked);
    svg.append(p);
    const what = t(a.key, a.vars);
    const li = h("li", { title: what }, svg, h("span", { class: "what", text: what }), h("span", { class: "when", text: ago(a.at) }));
    if (!(i === 0 && state.activityFresh)) li.style.animation = "none";
    return li;
  }));
  state.activityFresh = false;
}

function renderRailInfo() {
  const s = state.settings;
  if (!s) return;
  const mib = { standard: 128, high: 256, maximum: 512 }[s.strength] || 128;
  $("railKdf").textContent = t("rail.kdf", { mib });
  $("railAutolock").textContent = s.autolock_minutes ? t("rail.autolock", { n: s.autolock_minutes }) : t("rail.autolockOff");
  if (state.info) $("railVersion").textContent = `PrivaVault ${state.info.version}`;
}

function shake(form) {
  form.classList.remove("shake");
  void form.offsetWidth;
  form.classList.add("shake");
}

// ---------------------------------------------------------------- open / vault
function setPicked(info) {
  showMsg($("openMsg"), "");
  if (!info || !info.is_vault) {
    state.picked = null;
    $("pickedVault").hidden = true;
    $("openDrop").classList.remove("has");
    if (info) showMsg($("openMsg"), t("open.notVault"));
    return;
  }
  state.picked = info;
  $("pickedVault").hidden = false;
  $("pickedName").textContent = info.name;
  $("pickedMeta").textContent = `${fmtBytes(info.size)} · ${info.path}`;
  $("openDrop").classList.add("has");
  $("openPw").focus();
}

async function unlock(ev) {
  ev.preventDefault();
  const msg = $("openMsg");
  if (!state.picked) return showMsg(msg, t("err.no_vault"));
  const pw = $("openPw").value;
  if (!pw) return showMsg(msg, t("err.no_password"));
  try {
    const res = await runOp(() => invoke("open_vault", { path: state.picked.path, password: pw }));
    $("openPw").value = "";
    state.vault = res;
    state.lastActivity = Date.now();
    logActivity("unlocked", "act.unlocked", { name: res.name });
    $("vaultRows").closest("table").classList.add("animate");
    setTimeout(() => $("vaultRows").closest("table").classList.remove("animate"), 1200);
    $("vaultFilter").value = "";
    showMsg($("vaultMsg"), "");
    $("vaultResult").hidden = true;
    state.vaultResult = null;
    renderVault();
  } catch (e) {
    showMsg(msg, errText(e));
    if (e && e.code === "wrong_password") shake($("openPwForm"));
    $("openPw").select();
  }
}

function renderVault() {
  const v = state.vault;
  $("unlockPane").hidden = !!v;
  $("vaultPane").hidden = !v;
  $("sessionPill").hidden = !v;
  if (!v) return;
  $("sessionName").textContent = v.name;
  $("vaultName").textContent = v.name;
  $("vaultMeta").textContent = t("vault.meta", { format: t(`fmt.${v.format}`), size: fmtBytes(v.size), kdf: v.kdf });
  $("legacyNote").hidden = v.format !== "v1";
  const unsafeCount = v.entries.filter((e) => e.unsafe_path).length;
  $("unsafeNote").hidden = unsafeCount === 0;
  $("unsafeNote").textContent = t("vault.unsafe", { n: unsafeCount });
  renderRows();
  renderTimer();
}

function renderRows() {
  const v = state.vault;
  if (!v) return;
  const q = $("vaultFilter").value.trim().toLowerCase();
  const rows = v.entries.filter((e) => !q || e.path.toLowerCase().includes(q));
  $("vaultCount").textContent = t("vault.count", { n: rows.length, total: v.entries.length });
  $("vaultEmpty").hidden = rows.length > 0;
  $("vaultEmpty").textContent = v.entries.length === 0 ? t("vault.empty") : t("vault.noMatch", { q });
  $("vaultRows").replaceChildren(...rows.slice(0, 5000).map((e, i) => {
    const trimmed = e.path.replace(/[\\/]+$/, "");
    const cut = Math.max(trimmed.lastIndexOf("/"), trimmed.lastIndexOf("\\"));
    const dir = cut >= 0 ? trimmed.slice(0, cut + 1) : "";
    const base = cut >= 0 ? trimmed.slice(cut + 1) : trimmed;
    const name = h("div", { class: "entry-name", title: e.unsafe_path ? t("vault.unsafeEntry") : e.path },
      icon(e.is_dir ? "dir" : "file"),
      h("span", { class: "label" }, dir ? h("span", { class: "dir", text: dir }) : null, base));
    const action = e.is_dir || e.unsafe_path ? "" :
      h("button", { class: "btn", type: "button", text: t("vault.save"), "aria-label": `${t("vault.save")} ${base}`,
        onclick: () => extractOne(e.index) });
    return h("tr", { class: e.unsafe_path ? "unsafe" : "", vars: { "--i": Math.min(i, 30) } },
      h("td", {}, name),
      h("td", { class: "num", text: e.is_dir ? "" : fmtBytes(e.size) }),
      h("td", { class: "when", text: fmtDate(e.modified) }),
      h("td", { class: "act" }, action));
  }));
}

async function vaultOp(cmd, describe) {
  showMsg($("vaultMsg"), "");
  $("vaultResult").hidden = true;
  $("vaultResultText").textContent = "";
  state.vaultResult = null;
  state.lastActivity = Date.now();
  try {
    const res = await runOp(() => invoke(cmd));
    if (!res) return;
    setVaultResult(() => describe(res));
    logActivity("extracted", cmd === "export_zip" ? "act.zip" : "act.extracted", { n: res.files });
    notifyIfAway("PrivaVault", t("notify.decDone"));
  } catch (e) {
    if (e && e.code === "locked") lockUi();
    showMsg($("vaultMsg"), errText(e));
  }
}

function extractOne(index) {
  state.lastActivity = Date.now();
  showMsg($("vaultMsg"), "");
  $("vaultResult").hidden = true;
  $("vaultResultText").textContent = "";
  state.vaultResult = null;
  runOp(() => invoke("extract_entry", { index }))
    .then((res) => {
      if (!res) return;
      const name = res.output.split(/[\\/]/).pop();
      setVaultResult(() => t("vault.savedOne", { name }));
      logActivity("extracted", "act.saved", { name });
    })
    .catch((e) => showMsg($("vaultMsg"), errText(e)));
}

function lockUi() {
  state.vault = null;
  renderVault();
}

async function lockVault() {
  const name = state.vault?.name;
  await invoke("lock_vault").catch(() => {});
  lockUi();
  if (name) logActivity("closed", "act.closed", { name });
}

// ---------------------------------------------------------------- auto-lock
for (const evName of ["pointerdown", "keydown", "wheel"]) {
  window.addEventListener(evName, () => (state.lastActivity = Date.now()), { passive: true });
}

function renderTimer() {
  const mins = state.settings?.autolock_minutes ?? 10;
  if (!state.vault) return;
  if (!mins) { $("sessionTimer").textContent = t("vault.unlocked"); return; }
  const left = mins * 60 - (Date.now() - state.lastActivity) / 1000;
  $("sessionTimer").textContent = t("vault.lockIn", { t: fmtDuration(Math.max(0, left)) });
}

setInterval(() => {
  const mins = state.settings?.autolock_minutes ?? 10;
  if (state.vault && mins && !state.busy && Date.now() - state.lastActivity > mins * 60000) lockVault();
  renderTimer();
}, 5000);
setInterval(renderActivity, 30000);

// ---------------------------------------------------------------- settings
function fillSettings() {
  const s = state.settings, f = $("settingsForm");
  f.querySelector(`input[name=theme][value=${s.theme}]`).checked = true;
  f.querySelector(`input[name=strength][value=${s.strength}]`).checked = true;
  $("setLang").value = s.language;
  $("setExt").value = s.default_extension;
  $("setLock").value = String(s.autolock_minutes);
  if (!$("setLock").value) $("setLock").value = "10";
  f.check_updates.checked = s.check_updates;
  f.notifications.checked = s.notifications;
}

async function saveSettingsFromForm() {
  const f = $("settingsForm");
  const next = {
    ...state.settings,
    theme: f.querySelector("input[name=theme]:checked").value,
    strength: f.querySelector("input[name=strength]:checked").value,
    language: $("setLang").value,
    default_extension: $("setExt").value,
    autolock_minutes: parseInt($("setLock").value, 10) || 0,
    check_updates: f.check_updates.checked,
    notifications: f.notifications.checked,
  };
  try {
    const turnedOnUpdates = next.check_updates && !state.settings.check_updates;
    state.settings = await invoke("save_settings", { settings: next });
    applyTheme();
    applyI18n();
    showMsg($("setMsg"), t("set.saved"), true);
    setTimeout(() => ($("setMsg").hidden = true), 1500);
    if (turnedOnUpdates) checkUpdates(true);
  } catch (e) {
    showMsg($("setMsg"), errText(e));
  }
}

function renderAbout() {
  const i = state.info;
  if (!i) return;
  const env = i.environment;
  const platform = [env.os, env.session, env.desktop].filter(Boolean).join(" / ");
  const row = (k, v, code) => [h("dt", { text: t(k) }), h("dd", { class: code ? "code" : "", text: v })];
  $("aboutList").replaceChildren(
    ...row("set.version", i.version),
    ...row("set.platform", platform),
    ...row("set.crypto", t("set.cryptoVal")),
    ...row("set.config", i.config_dir || "—", true),
  );
}

// ---------------------------------------------------------------- update check (opt-in)
/** Ask the backend (Rust) for an update. `manual` = the user pressed "Check now" (consent). */
async function checkUpdates(manual = false) {
  if (storeManaged() || (!manual && !state.settings?.check_updates)) return null;
  try {
    const u = await invoke("check_update", { manual });
    state.update = u;
    if (u) {
      $("updateText").textContent = t("update.text", { v: u.version });
      $("updateInstall").hidden = !u.can_install;
      $("updateBanner").hidden = false;
    }
    return { ok: true, update: u };
  } catch (e) {
    return { ok: false, error: e };
  }
}

async function installUpdate() {
  $("btnCancel").hidden = true; // an install can't be interrupted halfway
  try {
    await runOp(() => invoke("install_update"));
    // On success the app restarts (Windows: the installer takes over).
  } catch (e) {
    $("updateText").textContent = errText(e);
  } finally {
    $("btnCancel").hidden = false;
  }
}

// ---------------------------------------------------------------- incoming paths
async function handlePaths(infos) {
  if (!infos.length) return;
  const vaults = infos.filter((i) => i.is_vault);
  if (vaults.length && (state.view === "open" || vaults.length === infos.length)) {
    if (state.vault) await lockVault();
    showView("open");
    setPicked(vaults[0]);
  } else {
    state.view = "encrypt";
  showView("encrypt");
  document.fonts?.ready.then(moveIndicator);
    addToSelection(infos);
  }
}

async function setupDragDrop() {
  const wv = TAURI.webview.getCurrentWebview();
  await wv.onDragDropEvent(async (ev) => {
    const zone = state.view === "open" ? $("openDrop") : $("encDrop");
    const type = ev.payload.type;
    if (type === "enter" || type === "over") zone.classList.add("over");
    else zone.classList.remove("over");
    if (type === "drop" && !state.busy && ev.payload.paths?.length) {
      const infos = await invoke("inspect_paths", { paths: ev.payload.paths });
      if (state.view === "open") setPicked(infos[0]);
      else if (state.view === "encrypt") addToSelection(infos);
      else handlePaths(infos);
    }
  });
}

// ---------------------------------------------------------------- wiring
function wire() {
  document.querySelectorAll(".nav-item").forEach((b) => b.addEventListener("click", () => showView(b.dataset.view)));
  document.querySelectorAll(".reveal").forEach((b) => {
    b.setAttribute("aria-pressed", "false");
    b.addEventListener("click", () => {
      const input = $(b.dataset.target);
      const show = input.type === "password";
      input.type = show ? "text" : "password";
      b.setAttribute("aria-pressed", String(show));
    });
  });

  $("btnAddFiles").addEventListener("click", async () => addToSelection(await invoke("pick_inputs", { folders: false })));
  $("btnAddFolder").addEventListener("click", async () => addToSelection(await invoke("pick_inputs", { folders: true })));
  $("btnClearSel").addEventListener("click", () => { state.selection = []; renderSelection(); });
  $("encPw").addEventListener("input", updateMeter);
  $("encPw2").addEventListener("input", updateMeter);
  $("encPwForm").addEventListener("submit", startEncrypt);
  $("encReveal").addEventListener("click", () => invoke("reveal_output").catch(() => {}));
  $("encAgain").addEventListener("click", () => { state.encResult = null; $("encResult").hidden = true; $("encForm").hidden = false; });

  $("btnPickVault").addEventListener("click", async () => { const v = await invoke("pick_vault"); if (v) setPicked(v); });
  $("openPwForm").addEventListener("submit", unlock);
  $("btnLock").addEventListener("click", lockVault);
  $("railLock").addEventListener("click", lockVault);
  $("vaultFilter").addEventListener("input", renderRows);
  $("btnExtractAll").addEventListener("click", () => vaultOp("extract_all", (r) => t("vault.savedAll", { files: r.files, size: fmtBytes(r.bytes) })));
  $("btnExportZip").addEventListener("click", () => vaultOp("export_zip", (r) => t("vault.savedZip", { files: r.files, size: fmtBytes(r.bytes) })));
  $("vaultReveal").addEventListener("click", () => invoke("reveal_output").catch(() => {}));

  $("btnCancel").addEventListener("click", () => invoke("cancel"));
  $("settingsForm").addEventListener("change", saveSettingsFromForm);
  $("settingsForm").addEventListener("submit", (e) => e.preventDefault());
  $("btnReleases").addEventListener("click", () => invoke("open_releases", { url: null }));
  $("updateOpen").addEventListener("click", () =>
    invoke("open_releases", { url: state.update ? `${state.info.releases_url}/tag/v${state.update.version}` : null }));
  $("updateInstall").addEventListener("click", installUpdate);
  $("btnCheckNow").addEventListener("click", async () => {
    $("btnCheckNow").disabled = true;
    $("checkResult").textContent = t("set.checking");
    const r = await checkUpdates(true);
    $("btnCheckNow").disabled = false;
    $("checkResult").textContent = !r.ok ? errText(r.error)
      : r.update ? t("update.text", { v: r.update.version })
      : t("set.upToDate", { v: state.info.version });
  });
  $("updateDismiss").addEventListener("click", () => ($("updateBanner").hidden = true));

  $("obDone").addEventListener("click", async () => {
    $("onboarding").close();
    state.settings = await invoke("save_settings", { settings: { ...state.settings, onboarding_done: true } });
  });
  $("cqStay").addEventListener("click", () => $("confirmQuit").close());
  $("cqQuit").addEventListener("click", () => invoke("quit_app"));

  $("railChange").addEventListener("click", () => showView("settings"));
  window.addEventListener("resize", moveIndicator);

  document.addEventListener("keydown", (e) => {
    if (e.key === "Escape" && state.busy) invoke("cancel");
    if (!(e.ctrlKey || e.metaKey) || e.altKey || state.busy || document.querySelector("dialog[open]")) return;
    const k = e.key.toLowerCase();
    if (k === "n") { e.preventDefault(); showView("encrypt"); }
    else if (k === "o") {
      e.preventDefault();
      showView("open");
      if (!state.vault) $("btnPickVault").click();
    } else if (k === "l" && state.vault) { e.preventDefault(); lockVault(); }
    else if (k === ",") { e.preventDefault(); showView("settings"); }
  });
}

async function init() {
  wire();
  [state.settings, state.info] = await Promise.all([invoke("get_settings"), invoke("app_info")]);
  state.systemDark = state.info.system_dark;
  for (const el of [document.querySelector('input[name="check_updates"]').closest("label"), $("btnCheckNow").parentElement])
    el.hidden = storeManaged();
  applyTheme();
  applyI18n();
  fillSettings();
  showView("encrypt");

  await listen("progress", (e) => { if (state.busy) setProgress(e.payload); });
  await listen("update-progress", (e) => {
    if (state.busy) setProgress({ ...e.payload, phase: `update_${e.payload.phase}` });
  });
  await listen("open-paths", async (e) => handlePaths(await invoke("inspect_paths", { paths: e.payload })));
  await listen("close-requested", () => $("confirmQuit").showModal());
  await listen("system-theme", (e) => { state.systemDark = e.payload; applyTheme(); });
  await setupDragDrop();

  handlePaths(await invoke("take_launch_paths"));
  if (!state.settings.onboarding_done) $("onboarding").showModal();
  checkUpdates();
}

document.addEventListener("DOMContentLoaded", () => {
  init().catch((e) => console.error("init failed", e));
});

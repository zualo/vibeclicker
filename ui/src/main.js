import "./style.css";
import "@fontsource-variable/geist";
import replaceElement from "lucide/dist/esm/replaceElement.js";
import ArrowUpRight from "lucide/dist/esm/icons/arrow-up-right.js";
import Check from "lucide/dist/esm/icons/check.js";
import CircleStop from "lucide/dist/esm/icons/circle-stop.js";
import Download from "lucide/dist/esm/icons/download.js";
import Github from "lucide/dist/esm/icons/github.js";
import Keyboard from "lucide/dist/esm/icons/keyboard.js";
import Play from "lucide/dist/esm/icons/play.js";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { openUrl } from "@tauri-apps/plugin-opener";
import { relaunch } from "@tauri-apps/plugin-process";
import { check } from "@tauri-apps/plugin-updater";

const GITHUB_PROFILE_URL = "https://github.com/zualo";
const previewQuery = import.meta.env.DEV ? new URLSearchParams(location.search) : new URLSearchParams();
const previewTheme = previewQuery.get("theme");
const previewInputMode = previewQuery.get("input");
const previewWidth = Number(previewQuery.get("width"));
if (previewWidth >= 360 && previewWidth <= 1000) {
  document.documentElement.style.width = `${previewWidth}px`;
  document.body.style.width = `${previewWidth}px`;
}
const defaults = {
  cps: 100,
  intervalMs: 10,
  rateEntryMode: "cps",
  hotkey: { modifiers: 0, keyCode: 0x75 },
  inputMode: "mouse",
  keyboardKey: 0x20,
  mouseButton: "left",
  mouseButtons: ["left"],
  clickType: "single",
  humanLikeBehavior: false,
  theme: "system",
  alwaysOnTop: false,
  transparencyEffects: false,
  trayBehavior: "never",
};
const config = { ...defaults, hotkey: { ...defaults.hotkey } };
let active = false;
let currentView = "main";
let lastRenderedContinuous = null;
let captureTarget = null;
let captureInProgress = false;
let suppressMouseAuxClick = false;
let saveTimer;
let toastTimer;
let toastHideTimer;
const $ = (selector) => document.querySelector(selector);
const $$ = (selector) => [...document.querySelectorAll(selector)];
const statusPill = $("#status-pill");
const actionButton = $("#start-stop");
const toast = $("#toast");
const appFrame = $(".app-frame");
const fadingHiddenTargets = new WeakMap();
const fadingHiddenTimers = new WeakMap();
const VISIBILITY_FADE_MS = 120;
const MIN_WINDOW_HEIGHT = 558;
let lastWindowHeight = null;

const iconDefinitions = { ArrowUpRight, Check, CircleStop, Download, Github, Keyboard, Play };
const refreshIcons = () => {
  document.querySelectorAll("[data-lucide]").forEach((element) => replaceElement(element, {
    nameAttr: "data-lucide",
    icons: iconDefinitions,
    attrs: { width: 16, height: 16, "stroke-width": 1.8 },
  }));
};

function applyTheme(theme) {
  const dark = theme === "dark" || (theme === "system" && matchMedia("(prefers-color-scheme: dark)").matches);
  document.documentElement.dataset.theme = dark ? "dark" : "light";
}

function notify(message, isError = false) {
  clearTimeout(toastTimer);
  clearTimeout(toastHideTimer);
  toast.textContent = message;
  toast.dataset.error = String(isError);
  toast.classList.remove("is-capture-prompt");
  toast.hidden = false;
  requestAnimationFrame(() => toast.classList.add("is-visible"));
  toastTimer = setTimeout(() => {
    toast.classList.remove("is-visible");
    toastHideTimer = setTimeout(() => {
      if (toast.classList.contains("is-visible")) return;
      if (captureTarget) {
        showCapturePrompt();
      } else {
        toast.hidden = true;
      }
    }, 220);
  }, 3200);
}

function showCapturePrompt() {
  clearTimeout(toastTimer);
  clearTimeout(toastHideTimer);
  toast.textContent = "Press Escape to cancel";
  toast.dataset.error = "false";
  toast.classList.add("is-capture-prompt");
  toast.hidden = false;
  requestAnimationFrame(() => toast.classList.add("is-visible"));
}

function setCaptureLayer(isActive) {
  appFrame.classList.toggle("is-capturing", isActive);
}

function dismissCapturePrompt() {
  if (!toast.classList.contains("is-capture-prompt")) return;
  clearTimeout(toastTimer);
  clearTimeout(toastHideTimer);
  toast.classList.remove("is-visible");
  toastHideTimer = setTimeout(() => {
    if (toast.classList.contains("is-visible")) return;
    toast.hidden = true;
    toast.classList.remove("is-capture-prompt");
  }, 220);
}

function setActive(next) {
  active = next;
  statusPill.dataset.state = active ? "active" : "idle";
  if (config.clickType === "continuous") {
    const enabled = config.continuousEnabled !== false;
    statusPill.setAttribute("aria-checked", String(enabled));
    statusPill.setAttribute("aria-label", enabled ? "Continuous mode enabled" : "Continuous mode disabled");
    statusPill.title = enabled ? (active ? "Continuous mode enabled and running" : "Continuous mode enabled") : "Continuous mode disabled";
  } else {
    statusPill.setAttribute("aria-label", active ? "Active" : "Idle");
    statusPill.title = active ? "Active" : "Idle";
  }
  actionButton.classList.toggle("is-stop", active);
  actionButton.innerHTML = `<i data-lucide="${active ? "circle-stop" : "play"}" aria-hidden="true"></i><span>${active ? "Stop clicking" : "Start clicking"}</span>`;
  refreshIcons();
}

function formatHotkey(hotkey) {
  const parts = [];
  if (hotkey.modifiers & 2) parts.push("Ctrl");
  if (hotkey.modifiers & 1) parts.push("Alt");
  if (hotkey.modifiers & 4) parts.push("Shift");
  if (hotkey.modifiers & 8) parts.push("Win");
  const key = hotkey.keyCode;
  let label;
  if (key === 0x05) label = "Mouse 4";
  else if (key === 0x06) label = "Mouse 5";
  else if (key >= 0x70 && key <= 0x87) label = `F${key - 0x6f}`;
  else if (key >= 0x41 && key <= 0x5a) label = String.fromCharCode(key);
  else if (key >= 0x30 && key <= 0x39) label = String.fromCharCode(key);
  else label = ({ 0x20: "Space", 0x1b: "Esc", 0x09: "Tab", 0x0d: "Enter", 0x21: "PgUp", 0x22: "PgDn", 0x23: "End", 0x24: "Home", 0x25: "←", 0x26: "↑", 0x27: "→", 0x28: "↓", 0x2d: "Insert", 0x2e: "Delete" })[key] ?? `Key ${key}`;
  parts.push(label);
  return parts.join(" + ");
}

function updateHotkeyBadge() {
  $("#hotkey-badge").textContent = formatHotkey(config.hotkey);
  $("#output-key-badge").textContent = formatHotkey({ modifiers: 0, keyCode: config.keyboardKey });
}

function setFadingHidden(element, shouldHide, animate = true) {
  if (fadingHiddenTargets.get(element) === shouldHide) {
    if (!animate) {
      clearTimeout(fadingHiddenTimers.get(element));
      fadingHiddenTimers.delete(element);
      element.hidden = shouldHide;
      element.classList.remove("is-fading-out");
      if (shouldHide) {
        element.setAttribute("aria-hidden", "true");
        element.inert = true;
      } else {
        element.removeAttribute("aria-hidden");
        element.inert = false;
      }
    }
    return;
  }
  fadingHiddenTargets.set(element, shouldHide);
  clearTimeout(fadingHiddenTimers.get(element));
  fadingHiddenTimers.delete(element);

  const reduceMotion = matchMedia("(prefers-reduced-motion: reduce)").matches;
  if (shouldHide) {
    element.setAttribute("aria-hidden", "true");
    element.inert = true;
    if (!animate || reduceMotion || element.hidden) {
      element.hidden = true;
      element.classList.remove("is-fading-out");
      return;
    }
    element.classList.add("is-fading-out");
    fadingHiddenTimers.set(element, setTimeout(() => {
      if (fadingHiddenTargets.get(element) !== true) return;
      element.hidden = true;
      element.classList.remove("is-fading-out");
      fadingHiddenTimers.delete(element);
    }, VISIBILITY_FADE_MS));
    return;
  }

  element.removeAttribute("aria-hidden");
  element.inert = false;
  if (!animate || reduceMotion) {
    element.hidden = false;
    element.classList.remove("is-fading-out");
    return;
  }
  if (element.hidden) {
    element.hidden = false;
    element.classList.add("is-fading-out");
    requestAnimationFrame(() => requestAnimationFrame(() => {
      if (fadingHiddenTargets.get(element) === false) element.classList.remove("is-fading-out");
    }));
  } else {
    // Reverse an in-progress fade without waiting for its hide timer.
    element.classList.remove("is-fading-out");
  }
}

function updateContinuousModeUI() {
  const continuous = config.clickType === "continuous";
  const animate = lastRenderedContinuous !== null
    && lastRenderedContinuous !== continuous
    && currentView === "main";
  lastRenderedContinuous = continuous;
  const doubleButton = $('[data-setting="clickType"] [data-value="double"]');
  doubleButton.hidden = config.inputMode === "keyboard";
  statusPill.disabled = !continuous;
  statusPill.setAttribute("aria-checked", String(!continuous || config.continuousEnabled !== false));
  statusPill.setAttribute("aria-label", continuous ? (config.continuousEnabled !== false ? "Continuous mode enabled" : "Continuous mode disabled") : (active ? "Active" : "Idle"));
  statusPill.title = continuous
    ? (config.continuousEnabled !== false ? (active ? "Continuous mode enabled and running" : "Continuous mode enabled") : "Continuous mode disabled")
    : (active ? "Active" : "Idle");
  setFadingHidden($("#hotkey-card"), false, animate);
  setFadingHidden(actionButton, currentView !== "main" || continuous, animate);
}

function renderInputMode() {
  $$('[data-input-mode]').forEach((button) => {
    const selected = button.dataset.inputMode === config.inputMode;
    button.classList.toggle("is-selected", selected);
    button.setAttribute("aria-selected", String(selected));
    button.tabIndex = selected ? 0 : -1;
  });
  $$('[data-input-panel]').forEach((panel) => { panel.hidden = panel.dataset.inputPanel !== config.inputMode; });
}

function measureMouseInputPanel() {
  const mousePanel = $('[data-input-panel="mouse"]');
  const wasHidden = mousePanel.hidden;
  mousePanel.hidden = false;
  const height = Math.ceil(mousePanel.querySelector(".mouse-input-content").getBoundingClientRect().height);
  mousePanel.hidden = wasHidden;
  if (height > 0) $(".config-card").style.setProperty("--input-panel-height", `${height}px`);
}

window.addEventListener("resize", measureMouseInputPanel);

function syncWindowHeight() {
  if (!window.__TAURI_INTERNALS__) return;
  requestAnimationFrame(() => {
    const root = document.documentElement;
    const body = document.body;
    const viewStack = $(".view-stack");
    const previous = {
      rootMinHeight: root.style.minHeight,
      bodyMinHeight: body.style.minHeight,
      frameMinHeight: appFrame.style.minHeight,
      stackFlex: viewStack.style.flex,
    };
    root.style.minHeight = "0";
    body.style.minHeight = "0";
    appFrame.style.minHeight = "0";
    viewStack.style.flex = "none";
    const height = Math.max(MIN_WINDOW_HEIGHT, Math.ceil(appFrame.scrollHeight));
    root.style.minHeight = previous.rootMinHeight;
    body.style.minHeight = previous.bodyMinHeight;
    appFrame.style.minHeight = previous.frameMinHeight;
    viewStack.style.flex = previous.stackFlex;
    if (height === lastWindowHeight) return;
    lastWindowHeight = height;
    invoke("set_window_content_height", { height }).catch(() => {});
  });
}

window.addEventListener("resize", syncWindowHeight);
new ResizeObserver(syncWindowHeight).observe(appFrame);

function selectInputMode(mode, moveFocus = false) {
  if (!['mouse', 'keyboard'].includes(mode)) return;
  if (mode === "keyboard" && config.clickType === "double") {
    config.clickType = "single";
    setSegmentValue(document.querySelector('[data-setting="clickType"]'), config.clickType);
  }
  captureTarget = null;
  setCaptureLayer(false);
  dismissCapturePrompt();
  $("#change-hotkey").disabled = false;
  $("#change-output-key").disabled = false;
  config.inputMode = mode;
  renderInputMode();
  measureMouseInputPanel();
  updateContinuousModeUI();
  if (moveFocus) $(`[data-input-mode="${mode}"]`).focus();
  queueSave();
  updateHotkeyBadge();
}

function intervalFromCps(cps) {
  return 1000 / cps;
}

function writeIntervalFields(intervalMs) {
  let remaining = intervalMs;
  const hours = Math.floor(remaining / 3_600_000);
  remaining -= hours * 3_600_000;
  const minutes = Math.floor(remaining / 60_000);
  remaining -= minutes * 60_000;
  const seconds = Math.floor(remaining / 1000);
  remaining -= seconds * 1000;
  $("#hours-input").value = String(hours);
  $("#minutes-input").value = String(minutes);
  $("#seconds-input").value = String(seconds);
  $("#milliseconds-input").value = remaining.toFixed(3).replace(/0+$/, "").replace(/\.$/, "") || "0";
}

function writeInterval(intervalMs) {
  config.intervalMs = Math.min(86_400_000, Math.max(0.5, intervalMs));
  writeIntervalFields(config.intervalMs);
}

function writeCps(cps) {
  config.cps = Math.min(2000, Math.max(1, cps));
  $("#cps-input").value = String(config.cps);
}

function selectRateEntryMode(mode, moveFocus = false) {
  if (!["cps", "interval"].includes(mode)) return;
  config.rateEntryMode = mode;
  $$('[data-rate-entry]').forEach((button) => {
    const selected = button.dataset.rateEntry === mode;
    button.setAttribute("aria-pressed", String(selected));
    button.tabIndex = selected ? 0 : -1;
  });
  $$('[data-rate-panel]').forEach((panel) => { panel.hidden = panel.dataset.ratePanel !== mode; });
  if (moveFocus) $(`[data-rate-entry="${mode}"]`).focus();
  queueSave();
}

function queueSave() {
  clearTimeout(saveTimer);
  saveTimer = setTimeout(() => {
    saveTimer = null;
    void saveSettingsNow();
  }, 260);
}

async function saveSettingsNow() {
  clearTimeout(saveTimer);
  saveTimer = null;
  try {
    await invoke("save_settings", { config: { ...config } });
  } catch (error) {
    const message = String(error);
    let userMessage = message;
    if (config.clickType === "continuous" && message.includes("Continuous mode is unavailable")) {
      config.clickType = "single";
      setSegmentValue(document.querySelector('[data-setting="clickType"]'), config.clickType);
      updateContinuousModeUI();
    }
    notify(userMessage, true);
  }
}

function setSegmentValue(group, value) {
  group.querySelectorAll("button").forEach((button) => {
    const selected = button.dataset.value === value;
    button.setAttribute("aria-pressed", String(selected));
    button.tabIndex = selected ? 0 : -1;
  });
}

function setView(view) {
  currentView = view;
  invoke("set_clicking_allowed", { allowed: view === "main" })
    .then((state) => setActive(state.active))
    .catch((error) => {
      if (window.__TAURI_INTERNALS__) notify(String(error), true);
    });
  $$("[data-view]").forEach((button) => {
    const selected = button.dataset.view === view;
    button.classList.toggle("is-selected", selected);
    button.tabIndex = selected ? 0 : -1;
    if (selected) button.setAttribute("aria-current", "page");
    else button.removeAttribute("aria-current");
  });
  $$("[data-panel]").forEach((panel) => { panel.hidden = panel.dataset.panel !== view; });
  updateContinuousModeUI();
}

async function loadSettings() {
  try {
    Object.assign(config, await invoke("get_settings"));
  } catch {
    // Browser preview uses defaults; the desktop build reads its saved config.
  }
  config.hotkey ??= { ...defaults.hotkey };
  config.mouseButton ??= defaults.mouseButton;
  config.mouseButtons ??= [config.mouseButton];
  if (!Array.isArray(config.mouseButtons) || !config.mouseButtons.length) config.mouseButtons = [config.mouseButton];
  config.clickType ??= defaults.clickType;
  config.humanLikeBehavior ??= false;
  config.continuousEnabled ??= true;
  if (!["single", "double", "continuous"].includes(config.clickType)) config.clickType = defaults.clickType;
  config.inputMode ??= defaults.inputMode;
  config.keyboardKey ??= defaults.keyboardKey;
  config.theme ??= defaults.theme;
  config.alwaysOnTop ??= false;
  config.transparencyEffects ??= false;
  config.trayBehavior ??= defaults.trayBehavior;
  config.rateEntryMode ??= defaults.rateEntryMode;
  if (!["cps", "interval"].includes(config.rateEntryMode)) config.rateEntryMode = defaults.rateEntryMode;
  if (["mouse", "keyboard"].includes(previewInputMode)) config.inputMode = previewInputMode;
  if (config.inputMode === "keyboard" && config.clickType === "double") {
    config.clickType = "single";
    queueSave();
  }
  writeInterval(Number(config.intervalMs) || defaults.intervalMs);
  writeCps(Number(config.cps) || defaults.cps);
  selectRateEntryMode(config.rateEntryMode);
  document.querySelectorAll('[data-setting="mouseButton"] button').forEach((button) => button.setAttribute("aria-pressed", String(config.mouseButtons.includes(button.dataset.value))));
  setSegmentValue(document.querySelector('[data-setting="clickType"]'), config.clickType);
  renderInputMode();
  measureMouseInputPanel();
  setSegmentValue(document.querySelector('[data-setting="theme"]'), config.theme);
  setSegmentValue(document.querySelector('[data-setting="trayBehavior"]'), config.trayBehavior);
  $("#always-on-top").setAttribute("aria-checked", String(config.alwaysOnTop));
  $("#transparency-effects").setAttribute("aria-checked", String(config.transparencyEffects));
  document.documentElement.dataset.transparencyEffects = String(config.transparencyEffects);
  applyTheme(["light", "dark", "system"].includes(previewTheme) ? previewTheme : config.theme);
  updateHotkeyBadge();
  updateContinuousModeUI();
  setSegmentValue(document.querySelector('[data-setting="humanLikeBehavior"]'), config.humanLikeBehavior ? "human" : "static");
}

$("#cps-input").addEventListener("change", (event) => {
  const value = Number(event.currentTarget.value);
  if (!Number.isFinite(value) || value <= 0) return writeCps(config.cps);
  writeCps(value);
  queueSave();
});

const timeInputs = ["#hours-input", "#minutes-input", "#seconds-input", "#milliseconds-input"].map($);
timeInputs.forEach((input) => input.addEventListener("change", () => {
  const [hours, minutes, seconds, milliseconds] = timeInputs.map((field) => Math.max(0, Number(field.value) || 0));
  const totalMs = Math.min(86_400_000, hours * 3_600_000 + minutes * 60_000 + seconds * 1000 + milliseconds);
  if (!Number.isFinite(totalMs) || totalMs < 0.5) {
    notify("Enter an interval greater than zero.", true);
    writeIntervalFields(config.intervalMs);
    return;
  }
  writeInterval(totalMs);
  queueSave();
}));

$$("[data-rate-entry]").forEach((button) => button.addEventListener("click", () => selectRateEntryMode(button.dataset.rateEntry)));
$(".rate-entry-switch").addEventListener("keydown", (event) => {
  const buttons = $$('[data-rate-entry]');
  const index = buttons.indexOf(document.activeElement);
  if (index < 0) return;
  let nextIndex;
  if (event.key === "ArrowRight") nextIndex = (index + 1) % buttons.length;
  else if (event.key === "ArrowLeft") nextIndex = (index - 1 + buttons.length) % buttons.length;
  else if (event.key === "Home") nextIndex = 0;
  else if (event.key === "End") nextIndex = buttons.length - 1;
  else return;
  event.preventDefault();
  selectRateEntryMode(buttons[nextIndex].dataset.rateEntry, true);
});

$$('[data-setting="mouseButton"] button, [data-setting="clickType"] button, [data-setting="humanLikeBehavior"] button').forEach((button) => button.addEventListener("click", () => {
  const group = button.closest("[data-setting]");
  const key = group.dataset.setting;
  if (key === "humanLikeBehavior") {
    config.humanLikeBehavior = button.dataset.value === "human";
    setSegmentValue(group, button.dataset.value);
  } else if (key === "mouseButton" && config.clickType === "continuous") {
    const next = config.mouseButtons.includes(button.dataset.value)
      ? config.mouseButtons.filter((value) => value !== button.dataset.value)
      : [...config.mouseButtons, button.dataset.value];
    if (!next.length) return;
    config.mouseButtons = next;
    config.mouseButton = next[0];
    group.querySelectorAll("button").forEach((item) => item.setAttribute("aria-pressed", String(next.includes(item.dataset.value))));
  } else {
    config[key] = button.dataset.value;
    if (key === "mouseButton") config.mouseButtons = [config.mouseButton];
    if (key === "clickType" && config.clickType !== "continuous") {
      config.mouseButtons = [config.mouseButtons[0] ?? config.mouseButton ?? "left"];
      config.mouseButton = config.mouseButtons[0];
      document.querySelectorAll('[data-setting="mouseButton"] button').forEach((item) => item.setAttribute("aria-pressed", String(item.dataset.value === config.mouseButton)));
    }
    setSegmentValue(group, config[key]);
  }
  if (key === "clickType" || key === "mouseButton") updateContinuousModeUI();
  if (key === "clickType" || (key === "mouseButton" && config.clickType === "continuous")) {
    void saveSettingsNow();
  } else {
    queueSave();
  }
}));

statusPill.addEventListener("click", async () => {
  if (config.clickType !== "continuous") return;
  const next = config.continuousEnabled === false;
  try { await invoke("set_continuous_enabled", { enabled: next }); config.continuousEnabled = next; updateContinuousModeUI(); }
  catch (error) { notify(String(error), true); }
});

$$('[data-setting="theme"] button').forEach((button) => button.addEventListener("click", () => {
  config.theme = button.dataset.value;
  setSegmentValue(button.closest("[data-setting]"), config.theme);
  applyTheme(config.theme);
  invoke("set_theme", { theme: config.theme }).catch((error) => notify(String(error), true));
}));

matchMedia("(prefers-color-scheme: dark)").addEventListener("change", () => {
  if (config.theme === "system") applyTheme("system");
});

$("#always-on-top").addEventListener("click", async (event) => {
  const toggle = event.currentTarget;
  const next = toggle.getAttribute("aria-checked") !== "true";
  try {
    await invoke("set_always_on_top", { enabled: next });
    config.alwaysOnTop = next;
    toggle.setAttribute("aria-checked", String(next));
  } catch (error) { notify(String(error), true); }
});

$("#transparency-effects").addEventListener("click", async (event) => {
  const toggle = event.currentTarget;
  const next = toggle.getAttribute("aria-checked") !== "true";
  toggle.disabled = true;
  try {
    await invoke("set_transparency_effects", { enabled: next });
    config.transparencyEffects = next;
    toggle.setAttribute("aria-checked", String(next));
    document.documentElement.dataset.transparencyEffects = String(next);
  } catch (error) { notify(String(error), true); }
  finally { toggle.disabled = false; }
});

$$('[data-setting="trayBehavior"] button').forEach((button) => button.addEventListener("click", () => {
  config.trayBehavior = button.dataset.value;
  setSegmentValue(button.closest("[data-setting]"), config.trayBehavior);
  queueSave();
}));

$("#change-hotkey").addEventListener("click", () => {
  captureTarget = "hotkey";
  setCaptureLayer(true);
  $("#change-hotkey").disabled = true;
  $("#change-output-key").disabled = true;
  showCapturePrompt();
});

$("#change-output-key").addEventListener("click", () => {
  captureTarget = "keyboardKey";
  setCaptureLayer(true);
  $("#change-output-key").disabled = true;
  $("#change-hotkey").disabled = true;
  showCapturePrompt();
});

async function captureInput(keyCode, modifiers = 0) {
  const target = captureTarget;
  if (!target || captureInProgress) return;
  const collision = config.inputMode === "keyboard"
    && (target === "hotkey"
      ? keyCode === config.keyboardKey
      : keyCode === config.hotkey.keyCode);
  if (collision) {
    notify("The repeated key and toggle hotkey must be different in every keyboard mode.", true);
    return;
  }
  const hotkey = { modifiers, keyCode };
  captureInProgress = true;
  updateHotkeyBadge();
  try {
    if (target === "hotkey") {
      await invoke("register_hotkey", { hotkey });
      config.hotkey = hotkey;
    } else {
      await invoke("set_keyboard_key", { keyCode: hotkey.keyCode });
      config.keyboardKey = hotkey.keyCode;
    }
  } catch (error) {
    notify(String(error), true);
    captureInProgress = false;
    updateHotkeyBadge();
    return;
  }
  captureTarget = null;
  setCaptureLayer(false);
  captureInProgress = false;
  dismissCapturePrompt();
  $("#change-hotkey").disabled = false;
  $("#change-output-key").disabled = false;
  updateHotkeyBadge();
  updateContinuousModeUI();
}

window.addEventListener("keydown", async (event) => {
  if (!captureTarget) return;
  event.preventDefault();
  event.stopPropagation();
  if (event.key === "Escape") {
    captureTarget = null;
    setCaptureLayer(false);
    dismissCapturePrompt();
    $("#change-hotkey").disabled = false;
    $("#change-output-key").disabled = false;
    updateHotkeyBadge();
    return;
  }
  if (["Control", "Alt", "Shift", "Meta"].includes(event.key)) return;
  if (!event.keyCode) {
    notify("That key cannot be captured here. Choose another key.", true);
    return;
  }
  const modifiers = (event.altKey ? 1 : 0) | (event.ctrlKey ? 2 : 0) | (event.shiftKey ? 4 : 0) | (event.metaKey ? 8 : 0);
  await captureInput(event.keyCode, modifiers);
});

window.addEventListener("pointerdown", (event) => {
  if (captureTarget !== "hotkey" || event.pointerType !== "mouse" || (event.button !== 3 && event.button !== 4)) return;
  suppressMouseAuxClick = true;
  setTimeout(() => { suppressMouseAuxClick = false; }, 500);
  event.preventDefault();
  event.stopImmediatePropagation();
  void captureInput(event.button === 3 ? 0x05 : 0x06);
}, true);

window.addEventListener("auxclick", (event) => {
  if (!suppressMouseAuxClick || (event.button !== 3 && event.button !== 4)) return;
  event.preventDefault();
  event.stopPropagation();
  suppressMouseAuxClick = false;
}, true);

actionButton.addEventListener("click", async () => {
  actionButton.disabled = true;
  try {
    const state = await invoke(active ? "stop_clicking" : "start_clicking");
    setActive(state.active);
  } catch (error) { notify(String(error), true); }
  finally { actionButton.disabled = false; }
});

$("#github-link").addEventListener("click", async () => {
  try { await openUrl(GITHUB_PROFILE_URL); }
  catch (error) { notify(`Could not open GitHub: ${error}`, true); }
});

$("#check-for-updates").addEventListener("click", async (event) => {
  const button = event.currentTarget;
  if (!window.__TAURI_INTERNALS__) {
    notify("Updates are available in the installed app.", true);
    return;
  }
  button.disabled = true;
  const label = button.querySelector("span");
  try {
    label.textContent = "Checking...";
    const update = await check();
    if (!update) {
      notify("You are up to date.");
      return;
    }
    const confirmed = window.confirm(`vibeclicker ${update.version} is available. Install it now?`);
    if (!confirmed) return;
    let downloaded = 0;
    let contentLength = 0;
    label.textContent = "Downloading...";
    await update.downloadAndInstall((event) => {
      if (event.event === "Started") {
        contentLength = event.data.contentLength || 0;
      } else if (event.event === "Progress") {
        downloaded += event.data.chunkLength;
        if (contentLength > 0) label.textContent = `Downloading ${Math.round(downloaded / contentLength * 100)}%`;
      } else if (event.event === "Finished") {
        label.textContent = "Installing...";
      }
    });
    label.textContent = "Restarting...";
    await relaunch();
  } catch (error) {
    label.textContent = "Check for updates";
    notify(`Update failed: ${error}`, true);
  } finally {
    button.disabled = false;
    label.textContent = "Check for updates";
  }
});

$(".tab-switch").addEventListener("click", (event) => {
  const button = event.target.closest("[data-view]");
  if (button) setView(button.dataset.view);
});

function handleChoiceArrowKeys(event, buttons, onSelect) {
  const index = buttons.indexOf(event.target.closest("button"));
  if (index < 0) return;
  let nextIndex;
  if (event.key === "ArrowRight") nextIndex = (index + 1) % buttons.length;
  else if (event.key === "ArrowLeft") nextIndex = (index - 1 + buttons.length) % buttons.length;
  else if (event.key === "Home") nextIndex = 0;
  else if (event.key === "End") nextIndex = buttons.length - 1;
  else return;
  event.preventDefault();
  const button = buttons[nextIndex];
  button.focus();
  onSelect(button);
}

$(".tab-switch").addEventListener("keydown", (event) => {
  const buttons = $$('[data-view]');
  handleChoiceArrowKeys(event, buttons, (button) => setView(button.dataset.view));
});

document.addEventListener("keydown", (event) => {
  const group = event.target.closest('[data-setting="mouseButton"], [data-setting="clickType"], [data-setting="humanLikeBehavior"], [data-setting="theme"], [data-setting="trayBehavior"]');
  if (!group) return;
  const buttons = [...group.querySelectorAll("button")].filter((button) => !button.hidden && !button.disabled);
  handleChoiceArrowKeys(event, buttons, (button) => button.click());
});

$(".input-mode-switch").addEventListener("click", (event) => {
  const button = event.target.closest("[data-input-mode]");
  if (!button) return;
  selectInputMode(button.dataset.inputMode);
});

$(".input-mode-switch").addEventListener("keydown", (event) => {
  const tabs = $$("[data-input-mode]");
  const index = tabs.indexOf(document.activeElement);
  if (index < 0) return;
  let nextIndex;
  if (event.key === "ArrowRight") nextIndex = (index + 1) % tabs.length;
  else if (event.key === "ArrowLeft") nextIndex = (index - 1 + tabs.length) % tabs.length;
  else if (event.key === "Home") nextIndex = 0;
  else if (event.key === "End") nextIndex = tabs.length - 1;
  else return;
  event.preventDefault();
  selectInputMode(tabs[nextIndex].dataset.inputMode, true);
});

listen("clicker://state", (event) => setActive(event.payload.active));
listen("clicker://continuous-enabled", (event) => { config.continuousEnabled = Boolean(event.payload); updateContinuousModeUI(); }).catch(() => {});
listen("clicker://rate", (event) => {
  const rate = Number(event.payload.measuredCps);
  statusPill.title = active ? `${rate.toFixed(1)} accepted inputs per second` : "Idle";
}).catch(() => {});
listen("clicker://error", (event) => notify(event.payload, true)).catch(() => {});

refreshIcons();
loadSettings();
setView(new URLSearchParams(location.search).get("view") === "settings" ? "settings" : "main");
syncWindowHeight();

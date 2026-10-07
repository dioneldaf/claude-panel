import "./styles.css";
import { connect, type Backend, type HookActionResult } from "./backend";
import { MASCOT_NAME, PRODUCT_NAME } from "./branding";
import { dustPuff, installDrag } from "./drag";
import { icon } from "./icons";
import { Mascot } from "./mascot";
import {
  formatElapsed,
  hooksHint,
  moodOf,
  needsBeat,
  rowTooltip,
  STATE_META,
  STATE_ORDER,
  hookResultText,
  summaryLine,
  type SessionView,
  type Snapshot,
} from "./model";
import { pixelText } from "./pixelfont";
import { PresenceDirector } from "./presence";

const $ = <T extends HTMLElement>(id: string) => document.getElementById(id) as T;

const stage = $("stage");
const wobble = $("wobble");
const panel = $("panel");
const rowsEl = $<HTMLUListElement>("rows");
const pillEl = $("pill");
const emptyEl = $("empty");
const summaryEl = $("summary");
const legendEl = $("legend");
const hooksEl = $("hooks");
const hintEl = $<HTMLButtonElement>("hint");
const fxEl = $("fx");
const btnMute = $<HTMLButtonElement>("btn-mute");

const motionQuery = window.matchMedia("(prefers-reduced-motion: reduce)");
const reducedMotion = () => motionQuery.matches;

/** Exit duration of a view before the mode switches; matches the CSS. */
const LEAVE_MS = 120;
const BEAT_MS = 300;
const ELAPSED_REFRESH_MS = 5000;
const CONFIRM_WINDOW_MS = 4000;

let backend: Backend;
let snapshot: Snapshot = { sessions: [], profiles: [], demo: false, listener_ok: true };
let mini = false;
let hookExe: string | null = null;
let hookResults: HookActionResult[] = [];
let modeTimer = 0;
let beatTimer = 0;
/** False while the panel is hidden for lack of sessions: no clocks, no drawing. */
let onScreen = false;
let beat = 0;

const mascot = new Mascot($<HTMLCanvasElement>("mascot"), reducedMotion);
const rows = new Map<string, HTMLLIElement>();

// ---------- static chrome ----------

function buildChrome(): void {
  document.title = PRODUCT_NAME;
  $("title").append(pixelText(PRODUCT_NAME, 2));
  $("title").setAttribute("aria-label", PRODUCT_NAME);
  $("mascot").title = MASCOT_NAME;
  const slash = icon("slash");
  slash.classList.add("slash");
  btnMute.append(icon("bell"), slash);
  $("btn-mini").append(icon("mini"));
  $("btn-hide").append(icon("hide"));
  $("btn-legend").append(icon("help"));
  $("btn-hooks").append(icon("plug"));
  $("demo-badge").append(pixelText("DEMO", 2));

  for (const state of STATE_ORDER) {
    const item = document.createElement("div");
    item.className = "legend-item";
    item.dataset.state = state;
    item.title = `${STATE_META[state].label}: ${STATE_META[state].description}`;
    const light = document.createElement("span");
    light.className = "light";
    item.append(light, pixelText(STATE_META[state].short, 2));
    legendEl.append(item);
  }
}

// ---------- rows ----------

function createRow(session: SessionView, index: number): HTMLLIElement {
  const row = document.createElement("li");
  row.className = "row";
  row.dataset.id = session.id;
  // Stagger the first appearance; cleared afterwards so hover feedback is immediate.
  row.style.setProperty("--i", String(Math.min(index, 8)));
  window.setTimeout(() => row.style.removeProperty("--i"), 700);
  row.innerHTML =
    '<span class="light"></span>' +
    '<span class="who"><span class="name"></span><span class="folder"></span></span>' +
    '<span class="what"><span class="state-label"></span><span class="elapsed"></span></span>';
  return row;
}

function updateRow(row: HTMLLIElement, session: SessionView, now: number): void {
  const changed = row.dataset.state !== session.state;
  const isNew = row.dataset.state === undefined;
  if (changed) {
    row.dataset.state = session.state;
    row.querySelector(".state-label")!.replaceChildren(pixelText(STATE_META[session.state].short, 2));
    if (!isNew && !reducedMotion()) {
      // State change: the light pops in four frames. One-shot, so WAAPI rather than a transition.
      row.querySelector(".light")!.animate([{ transform: "scale(1.9)" }, { transform: "scale(1)" }], {
        duration: 260,
        easing: "steps(4, jump-none)",
      });
    }
  }
  const name = row.querySelector<HTMLElement>(".name")!;
  if (name.textContent !== session.name) name.textContent = session.name;
  const folderText = `${session.folder} · ${session.profile}`;
  const folder = row.querySelector<HTMLElement>(".folder")!;
  if (folder.textContent !== folderText) folder.textContent = folderText;
  updateElapsed(row, session, now);
}

function updateElapsed(row: HTMLElement, session: SessionView, now: number): void {
  const text = formatElapsed(now - session.since);
  const el = row.querySelector<HTMLElement>(".elapsed")!;
  if (el.textContent !== text) el.textContent = text;
  row.title = rowTooltip(session, now);
}

function renderRows(now: number): void {
  const seen = new Set<string>();
  snapshot.sessions.forEach((session, index) => {
    seen.add(session.id);
    let row = rows.get(session.id);
    if (!row) {
      row = createRow(session, index);
      rows.set(session.id, row);
    }
    updateRow(row, session, now);
    // Keep DOM order equal to snapshot order without touching rows already in place.
    const expected = rowsEl.children[index];
    if (expected !== row) rowsEl.insertBefore(row, expected ?? null);
  });
  for (const [id, row] of rows) {
    if (seen.has(id)) continue;
    rows.delete(id);
    row.dataset.leaving = "";
    rowsEl.append(row);
    window.setTimeout(() => row.remove(), reducedMotion() ? 0 : 170);
  }
  emptyEl.hidden = snapshot.sessions.length > 0;
  rowsEl.hidden = snapshot.sessions.length === 0;
}

function renderPill(): void {
  const lights = snapshot.sessions.map((session) => {
    const light = document.createElement("span");
    light.className = "light";
    light.dataset.state = session.state;
    light.title = `${session.name}: ${STATE_META[session.state].label}`;
    return light;
  });
  pillEl.replaceChildren(...lights);
}

// ---------- footer, legend, hooks ----------

function renderFooter(): void {
  const hint = !snapshot.listener_ok
    ? "Hook port unavailable. Another instance may be running."
    : hooksHint(snapshot.profiles, snapshot.demo);
  hintEl.hidden = hint === null;
  hintEl.textContent = hint ?? "";
  hintEl.title = hint ? `${hint}\nSession states are coarse until hooks are installed. Select to manage hooks.` : "";
  $("demo-badge").hidden = !snapshot.demo;
}

function describeResult(result: HookActionResult): string {
  return hookResultText(result);
}

function renderHooks(): void {
  if (hooksEl.hidden) return;
  const armed = hooksEl.querySelector<HTMLElement>("[data-armed]")?.dataset.action;
  hooksEl.replaceChildren();

  const add = (tag: string, className: string, text: string) => {
    const el = document.createElement(tag);
    el.className = className;
    el.textContent = text;
    hooksEl.append(el);
    return el;
  };

  if (snapshot.demo) {
    add("p", "", "Hook management is not available in demo mode.");
    return;
  }
  const labels: Record<string, string> = {
    installed: "installed",
    broken: "broken",
    partial: "outdated",
    not_installed: "not installed",
  };
  for (const profile of snapshot.profiles) {
    const line = add("div", "profile", "");
    const name = document.createElement("span");
    name.textContent = profile.tag;
    name.title = profile.dir;
    const status = document.createElement("span");
    status.dataset.hooks = profile.hooks;
    status.textContent = labels[profile.hooks] ?? profile.hooks;
    line.append(name, status);
  }
  if (snapshot.profiles.length === 0) add("p", "", "No Claude Code configuration directory was found.");

  add(
    "p",
    "",
    hookExe
      ? "Installing adds one asynchronous entry per hook event to each settings.json. A timestamped backup is written first. Sessions apply the change after a restart."
      : "The cpanel-hook executable was not found next to the application. Hooks cannot be installed.",
  );

  const buttons = add("div", "buttons", "");
  const makeButton = (action: "install" | "remove", label: string, disabled: boolean) => {
    const button = document.createElement("button");
    button.type = "button";
    button.className = "text-btn";
    button.dataset.action = action;
    button.disabled = disabled;
    button.textContent = armed === action ? `Confirm ${action}` : label;
    if (armed === action) button.dataset.armed = "";
    buttons.append(button);
  };
  makeButton("install", "Install hooks", !hookExe || snapshot.profiles.length === 0);
  makeButton("remove", "Remove hooks", snapshot.profiles.every((p) => p.hooks === "not_installed"));
  if (snapshot.profiles.some((p) => p.hooks === "broken")) {
    add(
      "p",
      "result",
      "Some entries point to an executable that no longer exists, so Claude Code reports a hook error on every event. Install again to repair them, or remove them.",
    ).dataset.error = "";
  }

  for (const result of hookResults) {
    const line = add("p", "result", describeResult(result));
    if (!result.ok) line.dataset.error = "";
  }
}

/** Two-step confirmation: the first press arms the button, the second one acts. */
async function onHooksClick(event: MouseEvent): Promise<void> {
  const button = (event.target as Element).closest<HTMLButtonElement>("button[data-action]");
  if (!button || button.disabled) return;
  const action = button.dataset.action as "install" | "remove";
  if (button.dataset.armed === undefined) {
    hooksEl.querySelectorAll("[data-armed]").forEach((el) => el.removeAttribute("data-armed"));
    button.dataset.armed = "";
    button.textContent = `Confirm ${action}`;
    window.setTimeout(() => {
      if (button.isConnected && button.dataset.armed !== undefined) {
        button.removeAttribute("data-armed");
        renderHooks();
      }
    }, CONFIRM_WINDOW_MS);
    return;
  }
  button.removeAttribute("data-armed");
  button.disabled = true;
  try {
    hookResults = await backend.hooksAction(action);
  } catch (error) {
    hookResults = [
      { tag: "hooks", shared_with: [], file: "", ok: false, changed: false, backup: null, error: String(error) },
    ];
  }
  renderHooks();
}

// ---------- clocks ----------

/** One shared low-frequency beat drives every blinking light. */
function syncBeat(): void {
  const wanted = onScreen && needsBeat(snapshot.sessions) && !reducedMotion() && !document.hidden;
  if (wanted && !beatTimer) {
    beatTimer = window.setInterval(() => {
      beat = (beat + 1) % 4;
      document.documentElement.dataset.beat = String(beat);
    }, BEAT_MS);
  } else if (!wanted && beatTimer) {
    window.clearInterval(beatTimer);
    beatTimer = 0;
    delete document.documentElement.dataset.beat;
  }
}

function refreshElapsed(): void {
  if (mini || document.hidden || !onScreen) return;
  const now = Date.now();
  for (const session of snapshot.sessions) {
    const row = rows.get(session.id);
    if (row) updateElapsed(row, session, now);
  }
}

// ---------- render ----------

function render(): void {
  const now = Date.now();
  renderRows(now);
  renderPill();
  renderFooter();
  renderHooks();
  summaryEl.textContent = summaryLine(snapshot.sessions);
  const mood = moodOf(snapshot.sessions);
  mascot.setMood(mood);
  panel.toggleAttribute("data-alert", mood === "alert");
  syncBeat();
}

// ---------- mode ----------

function applyMode(): void {
  panel.dataset.mode = mini ? "mini" : "full";
  syncMascot();
}

function syncMascot(): void {
  mascot.setPaused(mini || document.hidden || !onScreen);
}

/**
 * Switches between the full panel and the pill. The current view leaves first,
 * then the other one enters (CSS @starting-style). Built on transitions, so a
 * second toggle mid-way simply retargets instead of restarting.
 */
function setMode(nextMini: boolean): void {
  if (nextMini === mini && !modeTimer) return;
  mini = nextMini;
  backend.setMini(mini);
  window.clearTimeout(modeTimer);
  if (reducedMotion()) {
    panel.removeAttribute("data-leaving");
    applyMode();
    return;
  }
  if ((panel.dataset.mode === "mini") === mini) {
    // Toggled back before the old view finished leaving: just bring it back.
    panel.removeAttribute("data-leaving");
    modeTimer = 0;
    return;
  }
  panel.dataset.leaving = "";
  modeTimer = window.setTimeout(() => {
    modeTimer = 0;
    panel.removeAttribute("data-leaving");
    applyMode();
  }, LEAVE_MS);
}

// ---------- window fitting ----------

function fitWindow(): void {
  const pad = parseFloat(getComputedStyle(stage).paddingLeft) || 0;
  backend.resizeWindow(Math.ceil(panel.offsetWidth + pad * 2), Math.ceil(panel.offsetHeight + pad * 2));
}

// ---------- wiring ----------

function setMuted(muted: boolean): void {
  btnMute.toggleAttribute("data-muted", muted);
  btnMute.setAttribute("aria-pressed", String(muted));
  const label = muted ? "Notifications muted. Select to enable." : "Notifications enabled. Select to mute.";
  btnMute.title = label;
  btnMute.setAttribute("aria-label", label);
}

async function start(): Promise<void> {
  buildChrome();
  backend = await connect();
  const initial = await backend.getState();
  snapshot = initial.snapshot;
  mini = initial.ui.mini;
  hookExe = initial.hook_exe;
  setMuted(initial.ui.muted);
  applyMode();
  render();

  // The window always hugs the panel; layout size ignores transforms, so drag
  // wobble and mode transitions never trigger a resize.
  new ResizeObserver(fitWindow).observe(panel);
  fitWindow();

  // The window stays hidden until the backend reports a session; the director
  // then sizes it, shows it and plays the entrance (and the exit later on).
  const director = new PresenceDirector({
    panel,
    carrier: $("carrier"),
    mascotEl: $("mascot"),
    mascot,
    isMini: () => mini,
    reducedMotion,
    prepare: async () => {
      render();
      fitWindow();
      // Give the resize a moment to land before the window is shown.
      await new Promise((resolve) => window.setTimeout(resolve, 90));
    },
    showWindow: () => backend.showWindow(),
    dust: (at) => {
      if (!reducedMotion()) dustPuff(fxEl, at);
    },
    done: (phase) => backend.presenceDone(phase),
    setActive: (active) => {
      onScreen = active;
      syncMascot();
      syncBeat();
    },
  });
  backend.onPresence((phase) => director.apply(phase));
  director.apply(initial.phase);

  backend.onSnapshot((next) => {
    snapshot = next;
    render();
  });
  backend.onUi((ui) => setMuted(ui.muted));
  backend.onToggleMini(() => setMode(!mini));

  btnMute.addEventListener("click", () => backend.setMuted(!btnMute.hasAttribute("data-muted")));
  $("btn-mini").addEventListener("click", () => setMode(true));
  $("btn-hide").addEventListener("click", () => backend.hideWindow());
  $("btn-legend").addEventListener("click", () => {
    legendEl.hidden = !legendEl.hidden;
    $("btn-legend").setAttribute("aria-pressed", String(!legendEl.hidden));
  });
  const toggleHooks = () => {
    hooksEl.hidden = !hooksEl.hidden;
    $("btn-hooks").setAttribute("aria-pressed", String(!hooksEl.hidden));
    hookResults = [];
    renderHooks();
  };
  $("btn-hooks").addEventListener("click", toggleHooks);
  hintEl.addEventListener("click", toggleHooks);
  hooksEl.addEventListener("click", (event) => void onHooksClick(event));

  panel.addEventListener("dblclick", (event) => {
    if ((event.target as Element).closest("button")) return;
    setMode(!mini);
  });

  const drag = installDrag(panel, wobble, $("mascot"), {
    startWindowDrag: () => backend.dragStart(),
    endWindowDrag: () => backend.dragEnd(),
    onDragStart: () => mascot.setOverride("dragged"),
    onShake: () => mascot.setOverride("dizzy"),
    onDrop: (wasDizzy) => {
      if (!reducedMotion()) dustPuff(fxEl, panel);
      if (wasDizzy) {
        // Still seeing stars for a moment after being put down.
        mascot.setOverride("dizzy", 1800);
      } else {
        mascot.setOverride("landing", 450);
      }
    },
    reducedMotion,
  });
  backend.onDragEnded(() => drag.dropFromBackend());

  window.setInterval(refreshElapsed, ELAPSED_REFRESH_MS);
  document.addEventListener("visibilitychange", () => {
    syncMascot();
    syncBeat();
  });
  motionQuery.addEventListener("change", () => {
    syncBeat();
    syncMascot();
  });
  document.addEventListener("contextmenu", (event) => event.preventDefault());
}

void start();

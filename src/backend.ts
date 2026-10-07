// Thin wrapper over the Tauri bridge. Outside Tauri (plain `pnpm dev` in a browser)
// it falls back to a local demo so the interface can still be worked on.

import type { SessionState, Snapshot } from "./model";
import type { Phase } from "./presence";

export interface UiPrefs {
  mini: boolean;
  muted: boolean;
}

export interface InitialState {
  snapshot: Snapshot;
  ui: UiPrefs;
  hook_exe: string | null;
  /** Whether the panel is hidden, entering, visible or exiting. */
  phase: Phase;
}

export interface HookActionResult {
  tag: string;
  /** Other profiles whose settings resolve to the same file. */
  shared_with: string[];
  file: string;
  ok: boolean;
  changed: boolean;
  backup: string | null;
  error: string | null;
}

export interface Backend {
  getState(): Promise<InitialState>;
  onSnapshot(handler: (snapshot: Snapshot) => void): void;
  onUi(handler: (ui: UiPrefs) => void): void;
  onToggleMini(handler: () => void): void;
  onDragEnded(handler: () => void): void;
  onPresence(handler: (phase: Phase) => void): void;
  presenceDone(phase: "entering" | "exiting"): void;
  resizeWindow(width: number, height: number): void;
  showWindow(): void;
  hideWindow(): void;
  setMini(mini: boolean): void;
  setMuted(muted: boolean): void;
  dragStart(): void;
  dragEnd(): void;
  hooksAction(action: "install" | "remove"): Promise<HookActionResult[]>;
}

const inTauri = "__TAURI_INTERNALS__" in window;

async function tauriBackend(): Promise<Backend> {
  const { invoke } = await import("@tauri-apps/api/core");
  const { listen } = await import("@tauri-apps/api/event");
  const fire = (command: string, args?: Record<string, unknown>) => {
    invoke(command, args).catch((error) => console.error(command, error));
  };
  return {
    getState: () => invoke<InitialState>("get_state"),
    onSnapshot: (handler) => void listen<Snapshot>("snapshot", (e) => handler(e.payload)),
    onUi: (handler) => void listen<UiPrefs>("ui", (e) => handler(e.payload)),
    onToggleMini: (handler) => void listen("toggle-mini", () => handler()),
    onDragEnded: (handler) => void listen("drag-ended", () => handler()),
    onPresence: (handler) => void listen<Phase>("presence", (e) => handler(e.payload)),
    presenceDone: (phase) => fire("presence_done", { phase }),
    resizeWindow: (width, height) => fire("resize_window", { width, height }),
    showWindow: () => fire("show_window"),
    hideWindow: () => fire("hide_window"),
    setMini: (mini) => fire("set_mini", { mini }),
    setMuted: (muted) => fire("set_muted", { muted }),
    dragStart: () => fire("drag_start"),
    dragEnd: () => fire("drag_end"),
    hooksAction: (action) => invoke<HookActionResult[]>("hooks_action", { action }),
  };
}

/** Browser-only stand-in mirroring the Rust demo mode. */
function browserBackend(): Backend {
  const states: SessionState[] = ["working", "needs_you", "done", "waiting_subagent", "waiting_process", "paused"];
  const names = ["storefront-12", "billing-api-03", "docs-site-41", "data-pipeline-07", "mobile-app-22", "infra-09"];
  const snapshot = (): Snapshot => {
    const step = Math.floor(Date.now() / 4000);
    return {
      demo: true,
      listener_ok: true,
      profiles: [],
      sessions: names.map((name, i) => ({
        id: `demo-${i}`,
        name,
        cwd: `C:\\work\\${name}`,
        folder: name.replace(/-\d+$/, ""),
        profile: i % 2 ? "claude-work" : "claude",
        pid: 1000 + i,
        state: states[(step + i) % states.length],
        since: step * 4000,
        raw_status: null,
        waiting_for: null,
        hooks_seen: true,
        subagents: 0,
        background: 0,
      })),
    };
  };
  let ui: UiPrefs = { mini: false, muted: false };
  let uiHandler: (ui: UiPrefs) => void = () => {};
  return {
    // `?intro` plays the entrance on load; by default the panel is simply there.
    getState: async () => ({
      snapshot: snapshot(),
      ui,
      hook_exe: null,
      phase: (new URLSearchParams(location.search).has("intro") ? "entering" : "visible") as Phase,
    }),
    onSnapshot: (handler) => void setInterval(() => handler(snapshot()), 1000),
    onUi: (handler) => void (uiHandler = handler),
    onToggleMini: () => {},
    onDragEnded: () => {},
    // `?cycle` replays the exit and entrance every few seconds in the browser preview.
    onPresence: (handler) => {
      if (!new URLSearchParams(location.search).has("cycle")) return;
      let visible = true;
      setInterval(() => {
        visible = !visible;
        handler(visible ? "entering" : "exiting");
      }, 5000);
    },
    presenceDone: () => {},
    resizeWindow: () => {},
    showWindow: () => {},
    hideWindow: () => {},
    setMini: (mini) => void (ui = { ...ui, mini }),
    setMuted: (muted) => {
      ui = { ...ui, muted };
      uiHandler(ui);
    },
    dragStart: () => {},
    dragEnd: () => {},
    hooksAction: async () => [],
  };
}

if (!inTauri) document.documentElement.classList.add("browser");

export function connect(): Promise<Backend> {
  return inTauri ? tauriBackend() : Promise.resolve(browserBackend());
}

// View model received from the Rust core, plus pure presentation helpers.

export type SessionState =
  | "working"
  | "needs_you"
  | "done"
  | "waiting_subagent"
  | "waiting_process"
  | "paused";

export interface SessionView {
  id: string;
  name: string;
  cwd: string;
  folder: string;
  profile: string;
  pid: number;
  state: SessionState;
  /** Epoch milliseconds at which the state was entered. */
  since: number;
  raw_status: string | null;
  waiting_for: string | null;
  hooks_seen: boolean;
  subagents: number;
  background: number;
}

export interface ProfileView {
  tag: string;
  dir: string;
  /** "installed" | "partial" | "not_installed" | "broken" (entries point to a missing executable) */
  hooks: string;
}

export interface Snapshot {
  sessions: SessionView[];
  profiles: ProfileView[];
  demo: boolean;
  listener_ok: boolean;
}

export interface StateMeta {
  /** Full label for tooltips and the legend. */
  label: string;
  /** Short label drawn with the pixel font. */
  short: string;
  description: string;
}

export const STATE_ORDER: SessionState[] = [
  "needs_you",
  "working",
  "waiting_subagent",
  "waiting_process",
  "done",
  "paused",
];

export const STATE_META: Record<SessionState, StateMeta> = {
  working: {
    label: "Working",
    short: "WORKING",
    description: "The model is generating or running tools.",
  },
  needs_you: {
    label: "Needs you",
    short: "NEEDS YOU",
    description: "A permission prompt or a question is pending.",
  },
  done: {
    label: "Done",
    short: "DONE",
    description: "The turn finished. The session awaits the next prompt.",
  },
  waiting_subagent: {
    label: "Waiting for subagent",
    short: "SUBAGENT",
    description: "The main agent is idle while subagents are still running.",
  },
  waiting_process: {
    label: "Waiting for process",
    short: "PROCESS",
    description: "The main agent is idle while a background process is still running.",
  },
  paused: {
    label: "Paused",
    short: "PAUSED",
    description: "The session has been idle for more than ten minutes.",
  },
};

/** Overall mood of the mascot, derived from all sessions. */
export type Mood = "idle" | "working" | "alert" | "sleeping";

export function moodOf(sessions: SessionView[]): Mood {
  if (sessions.some((s) => s.state === "needs_you")) return "alert";
  if (sessions.some((s) => s.state === "working" || s.state.startsWith("waiting_"))) return "working";
  if (sessions.some((s) => s.state === "done")) return "idle";
  return "sleeping";
}

/** Whether any light blinks or pulses, which is the only reason to run the beat clock. */
export function needsBeat(sessions: SessionView[]): boolean {
  return sessions.some((s) => s.state !== "done" && s.state !== "paused");
}

export function formatElapsed(ms: number): string {
  const seconds = Math.max(0, Math.floor(ms / 1000));
  if (seconds < 60) return `${seconds}s`;
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return `${minutes}m`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours}h`;
  return `${Math.floor(hours / 24)}d`;
}

export function summaryLine(sessions: SessionView[]): string {
  if (sessions.length === 0) return "No active sessions";
  const total = `${sessions.length} session${sessions.length === 1 ? "" : "s"}`;
  const count = (state: SessionState) => sessions.filter((s) => s.state === state).length;
  const urgent = count("needs_you");
  if (urgent > 0) return `${total}, ${urgent} needs you`;
  const working = count("working");
  if (working > 0) return `${total}, ${working} working`;
  return total;
}

export function rowTooltip(s: SessionView, now: number): string {
  const lines = [
    `${STATE_META[s.state].label} for ${formatElapsed(now - s.since)}`,
    s.cwd,
    `Profile: ${s.profile} · PID ${s.pid}`,
  ];
  if (s.waiting_for) lines.push(`Waiting for: ${s.waiting_for}`);
  if (s.subagents > 0) lines.push(`Subagents running: ${s.subagents}`);
  if (s.background > 0) lines.push(`Background tasks: ${s.background}`);
  if (s.raw_status) lines.push(`Registry status: ${s.raw_status}`);
  if (!s.hooks_seen) lines.push("No hook events received (registry only).");
  return lines.join("\n");
}

/** Outcome of a hook install / remove for one settings file. */
export interface HookResultLike {
  tag: string;
  shared_with: string[];
  ok: boolean;
  changed: boolean;
  backup: string | null;
  error: string | null;
}

export function hookResultText(result: HookResultLike): string {
  // Profiles sharing one settings file are handled once, under the first tag.
  const who =
    result.shared_with.length > 0
      ? `${result.tag} (shared with ${result.shared_with.join(", ")}; hook tag: ${result.tag})`
      : result.tag;
  if (!result.ok) return `${who}: failed. ${result.error ?? ""}`.trim();
  if (!result.changed) return `${who}: already up to date.`;
  return result.backup ? `${who}: updated. Backup: ${result.backup}` : `${who}: settings file created.`;
}

/** Subtle footer hint about profiles lacking hooks; null when there is nothing to say. */
export function hooksHint(profiles: ProfileView[], demo: boolean): string | null {
  if (demo) return null;
  const tags = (status: string) => profiles.filter((p) => p.hooks === status).map((p) => p.tag);
  const broken = tags("broken");
  if (broken.length > 0) return `Hooks broken (executable missing): ${broken.join(", ")}`;
  const missing = tags("not_installed");
  if (missing.length > 0) return `Hooks not installed: ${missing.join(", ")}`;
  const outdated = tags("partial");
  if (outdated.length > 0) return `Hooks outdated: ${outdated.join(", ")}`;
  return null;
}

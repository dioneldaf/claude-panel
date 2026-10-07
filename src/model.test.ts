import { describe, expect, it } from "vitest";
import {
  hookResultText,
  formatElapsed,
  hooksHint,
  moodOf,
  needsBeat,
  rowTooltip,
  STATE_META,
  summaryLine,
  type SessionState,
  type SessionView,
} from "./model";

function session(state: SessionState, extra: Partial<SessionView> = {}): SessionView {
  return {
    id: "id-" + state,
    name: "name",
    cwd: "C:\\work\\proj",
    folder: "proj",
    profile: "claude",
    pid: 1,
    state,
    since: 0,
    raw_status: "busy",
    waiting_for: null,
    hooks_seen: true,
    subagents: 0,
    background: 0,
    ...extra,
  };
}

describe("STATE_META", () => {
  it("describes every state with a distinct label", () => {
    const states: SessionState[] = ["working", "needs_you", "done", "waiting_subagent", "waiting_process", "paused"];
    const labels = states.map((s) => STATE_META[s].label);
    expect(new Set(labels).size).toBe(states.length);
    for (const s of states) expect(STATE_META[s].description.length).toBeGreaterThan(10);
  });
});

describe("moodOf", () => {
  it("sleeps when there is nothing to watch", () => {
    expect(moodOf([])).toBe("sleeping");
    expect(moodOf([session("paused"), session("paused")])).toBe("sleeping");
  });
  it("is alert when any session needs the user, regardless of the rest", () => {
    expect(moodOf([session("working"), session("needs_you"), session("paused")])).toBe("alert");
  });
  it("works when a session is working or waiting on something", () => {
    expect(moodOf([session("done"), session("working")])).toBe("working");
    expect(moodOf([session("waiting_subagent")])).toBe("working");
    expect(moodOf([session("waiting_process"), session("paused")])).toBe("working");
  });
  it("idles when sessions only await the next prompt", () => {
    expect(moodOf([session("done"), session("paused")])).toBe("idle");
  });
});

describe("formatElapsed", () => {
  it("uses the largest whole unit", () => {
    expect(formatElapsed(-5)).toBe("0s");
    expect(formatElapsed(0)).toBe("0s");
    expect(formatElapsed(59_999)).toBe("59s");
    expect(formatElapsed(60_000)).toBe("1m");
    expect(formatElapsed(59 * 60_000 + 59_000)).toBe("59m");
    expect(formatElapsed(3_600_000)).toBe("1h");
    expect(formatElapsed(23 * 3_600_000)).toBe("23h");
    expect(formatElapsed(49 * 3_600_000)).toBe("2d");
  });
});

describe("needsBeat", () => {
  it("is only required for blinking or pulsing lights", () => {
    expect(needsBeat([])).toBe(false);
    expect(needsBeat([session("done"), session("paused")])).toBe(false);
    expect(needsBeat([session("done"), session("working")])).toBe(true);
    expect(needsBeat([session("needs_you")])).toBe(true);
    expect(needsBeat([session("waiting_process")])).toBe(true);
  });
});

describe("hooksHint", () => {
  const profile = (tag: string, hooks: string) => ({ tag, dir: "C:/" + tag, hooks });
  it("is silent when every profile is installed or in demo mode", () => {
    expect(hooksHint([profile("claude", "installed")], false)).toBeNull();
    expect(hooksHint([profile("claude", "not_installed")], true)).toBeNull();
    expect(hooksHint([], false)).toBeNull();
  });
  it("names the profiles without hooks", () => {
    expect(hooksHint([profile("claude", "not_installed"), profile("claude-work", "installed")], false)).toBe(
      "Hooks not installed: claude",
    );
    expect(hooksHint([profile("a", "not_installed"), profile("b", "not_installed")], false)).toBe(
      "Hooks not installed: a, b",
    );
  });
  it("puts entries that point to a missing executable first", () => {
    expect(hooksHint([profile("a", "not_installed"), profile("b", "broken")], false)).toBe(
      "Hooks broken (executable missing): b",
    );
  });
  it("reports outdated entries separately", () => {
    expect(hooksHint([profile("a", "partial"), profile("b", "installed")], false)).toBe("Hooks outdated: a");
  });
});

describe("summaryLine", () => {
  it("counts sessions and puts the urgent ones first", () => {
    expect(summaryLine([])).toBe("No active sessions");
    expect(summaryLine([session("done")])).toBe("1 session");
    expect(summaryLine([session("working"), session("working"), session("done")])).toBe("3 sessions, 2 working");
    expect(summaryLine([session("working"), session("needs_you")])).toBe("2 sessions, 1 needs you");
  });
});

describe("rowTooltip", () => {
  it("lists state, folder, profile and counters", () => {
    const text = rowTooltip(session("waiting_subagent", { subagents: 2, since: 1000 }), 61_000);
    expect(text).toContain("Waiting for subagent");
    expect(text).toContain("for 1m");
    expect(text).toContain("C:\\work\\proj");
    expect(text).toContain("Profile: claude");
    expect(text).toContain("Subagents running: 2");
  });
  it("shows what the session is waiting for when the registry says so", () => {
    expect(rowTooltip(session("needs_you", { waiting_for: "permission" }), 0)).toContain("Waiting for: permission");
    expect(rowTooltip(session("needs_you"), 0)).not.toContain("Waiting for:");
  });
  it("mentions when the state is coarse because no hook event was seen", () => {
    expect(rowTooltip(session("working", { hooks_seen: false }), 0)).toContain("registry only");
    expect(rowTooltip(session("working"), 0)).not.toContain("registry only");
  });
});

describe("hookResultText", () => {
  const base = { tag: "claude", shared_with: [], ok: true, changed: true, backup: null, error: null };
  it("describes each outcome", () => {
    expect(hookResultText(base)).toBe("claude: settings file created.");
    expect(hookResultText({ ...base, backup: "C:/x/settings.json.cpanel-backup-1" })).toBe(
      "claude: updated. Backup: C:/x/settings.json.cpanel-backup-1",
    );
    expect(hookResultText({ ...base, changed: false })).toBe("claude: already up to date.");
    expect(hookResultText({ ...base, ok: false, changed: false, error: "file changed, retry: x" })).toBe(
      "claude: failed. file changed, retry: x",
    );
  });
  it("names the profiles that share the file and the tag that was written", () => {
    expect(hookResultText({ ...base, shared_with: ["claude-work"], changed: false })).toBe(
      "claude (shared with claude-work; hook tag: claude): already up to date.",
    );
  });
});

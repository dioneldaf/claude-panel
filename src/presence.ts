// Entrance and exit choreography.
//
// The backend decides *when* the panel is on screen (a small state machine with a
// debounce); this module only plays the matching sequence and reports back when it
// is over. Sequences are chains of short stepped animations (Web Animations API)
// guarded by a token, so either one can be interrupted at any point: the new
// sequence starts from whatever is currently on screen instead of from zero.
//
// Motion uses `clip-path` on the panel and the independent `translate` property on
// the mascot and the wrapper, so it never fights the drag springs, which write
// `transform`.

import type { Mascot } from "./mascot";

export type Phase = "hidden" | "entering" | "visible" | "exiting";

export interface PresenceDeps {
  panel: HTMLElement;
  /** Wrapper moved as a whole when the mascot carries the panel away. */
  carrier: HTMLElement;
  mascotEl: HTMLElement;
  mascot: Mascot;
  isMini(): boolean;
  reducedMotion(): boolean;
  /** Renders the current sessions and fits the window to them (window still hidden). */
  prepare(): Promise<void>;
  showWindow(): void;
  dust(at: HTMLElement): void;
  /** Tells the backend that the animation of `phase` has finished. */
  done(phase: "entering" | "exiting"): void;
  /** Clocks and timers only run while something is on screen. */
  setActive(active: boolean): void;
}

// Clip rectangles of the full panel, as `inset(top right bottom left)`.
const FULL = "inset(0px 0px 0px 0px)";
/** Just the mascot's corner of the frame. */
const CORNER = "inset(0px calc(100% - 80px) calc(100% - 64px) 0px)";
/** The header strip, full width. */
const HEADER = "inset(0px 0px calc(100% - 64px) 0px)";
/** Mini pill rolled up to nothing at its left edge. */
const SHUT = "inset(0px 100% 0px 0px)";

const HOME = "0px 0px";

class Interrupted extends Error {}

export class PresenceDirector {
  private token = 0;
  private running: Animation[] = [];
  private shown: Phase = "hidden";

  constructor(private readonly deps: PresenceDeps) {
    deps.panel.dataset.presence = "hidden";
    deps.setActive(false);
  }

  /** Reacts to a phase decided by the backend. */
  apply(phase: Phase): void {
    switch (phase) {
      case "entering":
        void this.play(() => this.enter(), "entering");
        break;
      case "exiting":
        void this.play(() => this.exit(), "exiting");
        break;
      case "visible":
        // Normally reached by our own entrance; this covers a backend timeout or
        // a page that loaded while the panel was already up.
        if (this.shown === "hidden") void this.play(() => this.appearAtOnce(), null);
        break;
      case "hidden":
        this.token++;
        this.reset("hidden");
        break;
    }
  }

  // ---------- plumbing ----------

  /**
   * Runs a sequence. An interruption is normal (a newer sequence took over). Any
   * other failure must not leave the panel half-animated or the backend waiting:
   * the end state is applied at once and reported.
   */
  private async play(sequence: () => Promise<void>, end: "entering" | "exiting" | null): Promise<void> {
    const token = this.token;
    try {
      await sequence();
    } catch (error) {
      if (error instanceof Interrupted) return;
      console.error("presence sequence failed", error);
      // Only clean up if no newer sequence has started since.
      if (end && this.token === token + 1) {
        this.reset(end === "entering" ? "visible" : "hidden");
        if (end === "entering") this.deps.showWindow();
        this.deps.done(end);
      }
    }
  }

  private check(token: number): void {
    if (token !== this.token) throw new Interrupted();
  }

  private async sleep(ms: number, token: number): Promise<void> {
    await new Promise((resolve) => window.setTimeout(resolve, ms));
    this.check(token);
  }

  /** One stepped animation that holds its end state until the sequence is reset. */
  private async run(
    el: HTMLElement,
    frames: Keyframe[],
    duration: number,
    steps: number,
    token: number,
  ): Promise<void> {
    // `jump-none` shows both the first and the last frame; it needs at least two steps.
    const easing = `steps(${Math.max(2, steps)}, jump-none)`;
    const animation = el.animate(frames, { duration, easing, fill: "forwards" });
    this.running.push(animation);
    try {
      await animation.finished;
    } catch {
      throw new Interrupted();
    }
    this.check(token);
  }

  /** What is on screen right now, read before cancelling the previous sequence. */
  private capture(): { clip: string; carrier: string; mascot: string; opacity: string } {
    const { panel, carrier, mascotEl } = this.deps;
    const value = (el: HTMLElement, prop: "clipPath" | "translate", fallback: string) => {
      const computed = getComputedStyle(el)[prop];
      return !computed || computed === "none" ? fallback : computed;
    };
    return {
      clip: value(panel, "clipPath", FULL),
      carrier: value(carrier, "translate", HOME),
      mascot: value(mascotEl, "translate", HOME),
      opacity: getComputedStyle(carrier).opacity,
    };
  }

  private cancelAll(): void {
    for (const animation of this.running) animation.cancel();
    this.running = [];
  }

  /** Drops every trace of a sequence and leaves the panel plainly shown or hidden. */
  private reset(state: "visible" | "hidden"): void {
    const { panel, mascot } = this.deps;
    this.cancelAll();
    panel.style.clipPath = "";
    delete panel.dataset.rows;
    if (state === "hidden") panel.dataset.presence = "hidden";
    else delete panel.dataset.presence;
    mascot.setOverride(null);
    this.shown = state;
    this.deps.setActive(state === "visible");
  }

  private rows(): HTMLElement[] {
    return [...this.deps.panel.querySelectorAll<HTMLElement>(".row")];
  }

  /** Shows or hides the rows one after another through their own CSS transition. */
  private staggerRows(hidden: boolean): void {
    const rows = this.rows();
    rows.forEach((row, i) => {
      const order = hidden ? rows.length - 1 - i : i;
      row.style.setProperty("--i", String(Math.min(order, 8)));
      window.setTimeout(() => row.style.removeProperty("--i"), 700);
    });
    if (hidden) this.deps.panel.dataset.rows = "hidden";
    else delete this.deps.panel.dataset.rows;
  }

  private lights(): HTMLElement[] {
    return [...this.deps.panel.querySelectorAll<HTMLElement>("#pill .light")];
  }

  // ---------- sequences ----------

  private async appearAtOnce(): Promise<void> {
    const token = ++this.token;
    await this.deps.prepare();
    this.check(token);
    this.reset("visible");
    this.deps.showWindow();
  }

  private async enter(): Promise<void> {
    const { panel, carrier, mascotEl, mascot } = this.deps;
    const wasHidden = this.shown === "hidden";
    const from = this.capture();
    const token = ++this.token;
    this.cancelAll();
    this.deps.setActive(true);
    this.shown = "entering";

    if (wasHidden) {
      // Nothing may flash before the first frame: clip and row state are set
      // while the window is still hidden.
      panel.style.clipPath = this.deps.isMini() ? SHUT : CORNER;
      panel.dataset.rows = "hidden";
      delete panel.dataset.presence;
      await this.deps.prepare();
      this.check(token);
      this.deps.showWindow();
    }

    if (this.deps.reducedMotion()) {
      panel.style.clipPath = "";
      delete panel.dataset.rows;
      if (wasHidden) await this.run(panel, [{ opacity: 0 }, { opacity: 1 }], 200, 4, token);
    } else if (!wasHidden) {
      // Turned around mid-exit: come straight back from wherever things are.
      mascot.setOverride("landing");
      await Promise.all([
        this.run(panel, [{ clipPath: from.clip }, { clipPath: FULL }], 280, 4, token),
        this.run(carrier, [{ translate: from.carrier, opacity: from.opacity }, { translate: HOME, opacity: 1 }], 280, 4, token),
        this.run(mascotEl, [{ translate: from.mascot }, { translate: HOME }], 280, 4, token),
      ]);
      this.staggerRows(false);
      for (const light of this.lights()) light.getAnimations().forEach((a) => a.cancel());
    } else if (this.deps.isMini()) {
      // Mini: the pill unrolls, then the lights pop in one by one. The pops are
      // scheduled up front so the lights stay dark while the pill opens.
      const lights = this.lights();
      lights.forEach((light, i) => {
        const pop = light.animate(
          [
            { transform: "scale(0.4)", opacity: 0 },
            { transform: "scale(1.7)", opacity: 1, offset: 0.5 },
            { transform: "scale(1)", opacity: 1 },
          ],
          { duration: 220, delay: 320 + i * 60, easing: "steps(4, jump-none)", fill: "backwards" },
        );
        this.running.push(pop);
      });
      await this.run(panel, [{ clipPath: SHUT }, { clipPath: FULL }], 320, 5, token);
      await this.sleep(220 + lights.length * 60, token);
    } else {
      // 1. The mascot rises into its corner and looks around.
      mascot.setOverride("peek");
      await this.run(mascotEl, [{ translate: "0px 54px" }, { translate: HOME }], 420, 6, token);
      // 2. It greets.
      mascot.setOverride("wave");
      await this.sleep(600, token);
      // 3. It pulls the panel out: first across, then down.
      mascot.setOverride("pull");
      await this.run(panel, [{ clipPath: CORNER }, { clipPath: HEADER }], 360, 6, token);
      await this.run(panel, [{ clipPath: HEADER }, { clipPath: FULL }], 320, 6, token);
      // 4. The rows drop in and the mascot settles.
      mascot.setOverride("landing");
      this.staggerRows(false);
      await this.sleep(280, token);
    }

    this.reset("visible");
    this.deps.done("entering");
  }

  private async exit(): Promise<void> {
    const { panel, carrier, mascotEl, mascot } = this.deps;
    if (this.shown === "hidden") {
      // Nothing on screen (the page loaded mid-exit): just confirm.
      this.token++;
      this.deps.done("exiting");
      return;
    }
    const interrupted = this.shown === "entering";
    const from = this.capture();
    const token = ++this.token;
    this.cancelAll();
    this.shown = "exiting";
    panel.style.clipPath = "";

    if (this.deps.reducedMotion()) {
      await this.run(panel, [{ opacity: 1 }, { opacity: 0 }], 200, 4, token);
    } else if (this.deps.isMini()) {
      // Mini: the lights go out from the right, then the pill rolls shut.
      const lights = this.lights().reverse();
      lights.forEach((light, i) => {
        this.running.push(
          light.animate([{ opacity: 1 }, { opacity: 0 }], { duration: 120, delay: i * 70, easing: "steps(2, jump-none)", fill: "both" }),
        );
      });
      await this.sleep(140 + lights.length * 70, token);
      await this.run(panel, [{ clipPath: from.clip }, { clipPath: SHUT }], 280, 5, token);
      this.deps.dust(panel);
      await this.sleep(160, token);
    } else {
      if (interrupted) {
        // The mascot may still be rising: bring it home while packing.
        void this.run(mascotEl, [{ translate: from.mascot }, { translate: HOME }], 200, 3, token).catch(() => {});
        // Mid-entrance: skip the farewell and pack whatever is already out.
        mascot.setOverride("pull");
        await this.run(panel, [{ clipPath: from.clip }, { clipPath: CORNER }], 300, 5, token);
      } else {
        // 1. A wave goodbye while the rows leave, last one first.
        mascot.setOverride("wave");
        this.staggerRows(true);
        await this.sleep(560, token);
        // 2. The panel is rolled up: bottom first, then sideways into the corner.
        mascot.setOverride("pull");
        await this.run(panel, [{ clipPath: from.clip }, { clipPath: HEADER }], 300, 5, token);
        await this.run(panel, [{ clipPath: HEADER }, { clipPath: CORNER }], 300, 5, token);
      }
      // 3. The mascot hops away with the bundle, leaving a puff of dust.
      mascot.setOverride("carry");
      this.deps.dust(mascotEl);
      await this.run(
        carrier,
        [
          { translate: "0px 0px", opacity: 1 },
          { translate: "22px -16px", opacity: 1 },
          { translate: "44px 0px", opacity: 1 },
          { translate: "74px -18px", opacity: 1 },
          { translate: "104px 0px", opacity: 1 },
          { translate: "146px -20px", opacity: 0.6 },
          { translate: "196px 4px", opacity: 0 },
        ],
        660,
        13,
        token,
      );
    }

    this.reset("hidden");
    this.deps.done("exiting");
  }
}

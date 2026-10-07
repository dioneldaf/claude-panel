// The mascot, drawn as original pixel art on a 20x16 canvas that CSS scales up 3x.
// Frames are computed from a tick counter; a timer runs only while the pose
// actually animates and the mascot is visible.

import type { Mood } from "./model";

export type Pose = Mood | "dragged" | "dizzy" | "landing" | "peek" | "wave" | "pull" | "carry";

export const W = 20;
export const H = 16;

const COLOR = {
  body: "#d97757",
  shade: "#b85f43",
  dark: "#2b1b17",
  red: "#ff5a4f",
  yellow: "#f2c94c",
  light: "#f2e9dc",
  dim: "#9a8fa3",
  key: "#3a3346",
};

/** Milliseconds per frame for each pose. */
const TICK: Record<Pose, number> = {
  idle: 500,
  working: 140,
  alert: 130,
  sleeping: 700,
  dragged: 140,
  dizzy: 130,
  landing: 90,
  peek: 170,
  wave: 120,
  pull: 110,
  carry: 80,
};

type Eyes = "open" | "blink" | "closed" | "down" | "wide" | "left" | "right" | "dizzy-a" | "dizzy-b";

export interface Frame {
  /** Vertical offset of the body; positive compresses the legs, negative lifts off. */
  dy: number;
  /** Wide and low body (landing, sleeping). */
  squash: boolean;
  eyes: Eyes;
  /** Vertical offset of each arm. */
  armL: number;
  armR: number;
  /** Leg lengths, left to right. */
  legs: [number, number, number, number];
  extras: Array<[number, number, string]>;
}

const STAND: Frame["legs"] = [2, 2, 2, 2];

export function frameFor(pose: Pose, t: number): Frame {
  const base: Frame = { dy: 0, squash: false, eyes: "open", armL: 0, armR: 0, legs: STAND, extras: [] };
  switch (pose) {
    case "idle": {
      const phase = t % 12;
      const eyes: Eyes = phase === 7 ? "blink" : phase === 9 ? "right" : phase === 10 ? "left" : "open";
      return { ...base, dy: t % 4 >= 2 ? 1 : 0, eyes };
    }
    case "working": {
      const extras: Frame["extras"] = [];
      // Keyboard with one key lighting up per frame.
      for (let x = 3; x <= 16; x++) extras.push([x, 15, COLOR.key]);
      extras.push([4 + ((t * 5) % 12), 15, COLOR.light]);
      if (t % 3 !== 2) extras.push([t % 2 ? 1 : 18, 6 - (t % 3), COLOR.yellow]);
      return { ...base, dy: t % 4 === 3 ? 1 : 0, eyes: "down", armL: t % 2 ? 3 : 2, armR: t % 2 ? 2 : 3, extras };
    }
    case "alert": {
      const jump = [0, -1, -2, -2, -1, 0, 0, 0][t % 8];
      const extras: Frame["extras"] = [];
      if (t % 4 !== 3) {
        for (let y = 0; y <= 3; y++) extras.push([18, y, COLOR.red], [19, y, COLOR.red]);
        extras.push([18, 5, COLOR.red], [19, 5, COLOR.red]);
      }
      return { ...base, dy: jump, eyes: "wide", armL: -2, armR: -2, extras };
    }
    case "sleeping": {
      const phase = t % 4;
      const extras: Frame["extras"] = [];
      const z = (x: number, y: number, color: string) => {
        extras.push([x, y, color], [x + 1, y, color], [x + 2, y, color], [x + 1, y + 1, color]);
        extras.push([x, y + 2, color], [x + 1, y + 2, color], [x + 2, y + 2, color]);
      };
      if (phase === 1) extras.push([15, 5, COLOR.light], [16, 5, COLOR.light], [15, 6, COLOR.light]);
      if (phase === 2) z(16, 2, COLOR.light);
      if (phase === 3) z(17, 0, COLOR.dim);
      return { ...base, squash: true, dy: t % 4 >= 2 ? 1 : 0, eyes: "closed", legs: [1, 1, 1, 1], extras };
    }
    case "dragged": {
      const a = t % 2 === 0;
      return { ...base, dy: -1, eyes: "wide", armL: a ? -2 : -1, armR: a ? -1 : -2, legs: a ? [3, 2, 3, 2] : [2, 3, 2, 3] };
    }
    case "dizzy": {
      const orbit: Array<[number, number]> = [
        [5, 2],
        [8, 1],
        [11, 1],
        [14, 2],
        [11, 3],
        [8, 3],
      ];
      const [ax, ay] = orbit[t % 6];
      const [bx, by] = orbit[(t + 3) % 6];
      return {
        ...base,
        dy: t % 2,
        eyes: t % 2 ? "dizzy-a" : "dizzy-b",
        armL: 1,
        armR: 1,
        extras: [
          [ax, ay, COLOR.yellow],
          [bx, by, COLOR.light],
        ],
      };
    }
    case "peek":
      // Arriving: looks left and right before stepping out.
      return { ...base, eyes: t % 4 < 2 ? "left" : "right" };
    case "wave":
      // One arm up, swinging.
      return { ...base, dy: t % 4 === 0 ? 1 : 0, armR: t % 2 ? -4 : -3, eyes: t % 6 === 5 ? "blink" : "open" };
    case "pull":
      // Straining at something heavy: braced, arms working in turn.
      return { ...base, dy: t % 2, eyes: "closed", armL: t % 2 ? 0 : -1, armR: t % 2 ? -1 : 0 };
    case "carry": {
      // Hopping along with the load held overhead.
      const hop = [0, -2, -3, -2][t % 4];
      return { ...base, dy: hop, eyes: "open", armL: -3, armR: -3, legs: hop < 0 ? [1, 2, 2, 1] : STAND };
    }
    case "landing":
      return { ...base, squash: t < 2, dy: t < 2 ? 0 : t === 2 ? -1 : 0, eyes: t < 3 ? "closed" : "open", legs: t < 2 ? [1, 1, 1, 1] : STAND };
  }
}

/** Minimal drawing surface, so frames can be rendered without a real canvas. */
export interface PixelSurface {
  fillStyle: string | CanvasGradient | CanvasPattern;
  clearRect(x: number, y: number, w: number, h: number): void;
  fillRect(x: number, y: number, w: number, h: number): void;
}

export function draw(ctx: PixelSurface, f: Frame): void {
  ctx.clearRect(0, 0, W, H);
  const px = (x: number, y: number, w: number, h: number, color: string) => {
    ctx.fillStyle = color;
    ctx.fillRect(x, y, w, h);
  };

  // Feet rest on row 13; the body sits on top of the legs.
  const ground = 14;
  const lift = Math.min(f.dy, 0);
  const compress = Math.max(f.dy, 0);
  const bodyX = f.squash ? 4 : 5;
  const bodyW = f.squash ? 12 : 10;
  // A squashed body keeps its short legs and breathes by changing height instead.
  const bodyH = f.squash ? 6 - compress : 8;
  const legTop = f.squash ? ground - 1 + lift : ground - 2 + compress + lift;
  const bodyY = legTop - bodyH;

  const legXs = f.squash ? [4, 6, 13, 15] : [5, 7, 12, 14];
  legXs.forEach((x, i) => {
    const length = f.squash ? 1 : Math.max(1, f.legs[i] - compress);
    px(x, legTop, 1, length, COLOR.body);
  });

  px(bodyX, bodyY, bodyW, bodyH, COLOR.body);
  px(bodyX, bodyY + bodyH - 1, bodyW, 1, COLOR.shade);

  const armY = bodyY + (f.squash ? 2 : 4);
  px(bodyX - 2, armY + f.armL, 2, 2, COLOR.body);
  px(bodyX + bodyW, armY + f.armR, 2, 2, COLOR.body);

  const eyeY = bodyY + 2;
  const leftX = bodyX + 2;
  const rightX = bodyX + bodyW - 3;
  switch (f.eyes) {
    case "open":
      px(leftX, eyeY, 1, 2, COLOR.dark);
      px(rightX, eyeY, 1, 2, COLOR.dark);
      break;
    case "blink":
      px(leftX, eyeY + 1, 1, 1, COLOR.dark);
      px(rightX, eyeY + 1, 1, 1, COLOR.dark);
      break;
    case "closed":
      px(leftX, eyeY + 1, 2, 1, COLOR.dark);
      px(rightX - 1, eyeY + 1, 2, 1, COLOR.dark);
      break;
    case "down":
      px(leftX, eyeY + 1, 1, 2, COLOR.dark);
      px(rightX, eyeY + 1, 1, 2, COLOR.dark);
      break;
    case "wide":
      px(leftX, eyeY, 2, 2, COLOR.dark);
      px(rightX - 1, eyeY, 2, 2, COLOR.dark);
      break;
    case "left":
      px(leftX - 1, eyeY, 1, 2, COLOR.dark);
      px(rightX - 1, eyeY, 1, 2, COLOR.dark);
      break;
    case "right":
      px(leftX + 1, eyeY, 1, 2, COLOR.dark);
      px(rightX + 1, eyeY, 1, 2, COLOR.dark);
      break;
    case "dizzy-a":
      px(leftX, eyeY, 1, 1, COLOR.dark);
      px(leftX + 1, eyeY + 1, 1, 1, COLOR.dark);
      px(rightX - 1, eyeY, 1, 1, COLOR.dark);
      px(rightX, eyeY + 1, 1, 1, COLOR.dark);
      break;
    case "dizzy-b":
      px(leftX + 1, eyeY, 1, 1, COLOR.dark);
      px(leftX, eyeY + 1, 1, 1, COLOR.dark);
      px(rightX, eyeY, 1, 1, COLOR.dark);
      px(rightX - 1, eyeY + 1, 1, 1, COLOR.dark);
      break;
  }

  for (const [x, y, color] of f.extras) px(x, y, 1, 1, color);
}

export class Mascot {
  private readonly ctx: CanvasRenderingContext2D;
  private mood: Mood = "sleeping";
  private override: Pose | null = null;
  private overrideTimer = 0;
  private tick = 0;
  private timer = 0;
  private paused = false;

  constructor(
    canvas: HTMLCanvasElement,
    private readonly reducedMotion: () => boolean,
  ) {
    this.ctx = canvas.getContext("2d")!;
    this.ctx.imageSmoothingEnabled = false;
    this.restart();
  }

  get pose(): Pose {
    return this.override ?? this.mood;
  }

  setMood(mood: Mood): void {
    if (mood === this.mood) return;
    this.mood = mood;
    if (!this.override) this.restart();
  }

  /** Shows a temporary pose; `durationMs` omitted means until cleared. */
  setOverride(pose: Pose | null, durationMs?: number): void {
    window.clearTimeout(this.overrideTimer);
    this.override = pose;
    if (pose && durationMs) {
      this.overrideTimer = window.setTimeout(() => this.setOverride(null), durationMs);
    }
    this.restart();
  }

  /** Stops the frame timer while the mascot is not visible. */
  setPaused(paused: boolean): void {
    if (paused === this.paused) return;
    this.paused = paused;
    this.restart();
  }

  private restart(): void {
    window.clearTimeout(this.timer);
    this.tick = 0;
    draw(this.ctx, frameFor(this.pose, 0));
    if (!this.paused && !this.reducedMotion()) this.schedule();
  }

  private schedule(): void {
    this.timer = window.setTimeout(() => {
      this.tick++;
      draw(this.ctx, frameFor(this.pose, this.tick));
      this.schedule();
    }, TICK[this.pose]);
  }
}

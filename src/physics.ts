// Small physics helpers for the drag interaction. Pure, no DOM.

/** Longest simulated time per step call; longer frames are treated as a hitch. */
const MAX_STEP = 0.1;
/** Integration sub-step; keeps stiff springs stable at any frame rate. */
const SUB_STEP = 1 / 120;

/**
 * Damped spring. Unlike a CSS keyframe animation it keeps its velocity when the
 * target changes, so interrupted motion continues smoothly instead of restarting.
 */
export class Spring {
  value = 0;
  velocity = 0;
  target = 0;

  constructor(
    private readonly stiffness: number,
    private readonly damping: number,
  ) {}

  /** Advances the simulation by `dt` seconds. */
  step(dt: number): void {
    let remaining = Math.min(Math.max(dt, 0), MAX_STEP);
    while (remaining > 0) {
      const h = Math.min(remaining, SUB_STEP);
      const acceleration = (this.target - this.value) * this.stiffness - this.velocity * this.damping;
      this.velocity += acceleration * h;
      this.value += this.velocity * h;
      remaining -= h;
    }
  }

  impulse(velocity: number): void {
    this.velocity += velocity;
  }

  atRest(epsilon = 0.01): boolean {
    return Math.abs(this.target - this.value) < epsilon && Math.abs(this.velocity) < epsilon;
  }

  /** Jumps to the target without motion. */
  settle(): void {
    this.value = this.target;
    this.velocity = 0;
  }
}

/**
 * Detects shaking: several fast horizontal direction reversals in a short window.
 */
export class ShakeDetector {
  private lastSign = 0;
  private reversals: number[] = [];

  constructor(
    private readonly windowMs = 900,
    /** Minimum speed in pixels per millisecond for a sample to count. */
    private readonly minSpeed = 0.5,
    private readonly needed = 4,
  ) {}

  /** Feeds a horizontal velocity sample; returns true while the shake threshold is met. */
  push(vx: number, timeMs: number): boolean {
    if (Math.abs(vx) < this.minSpeed) return false;
    const sign = Math.sign(vx);
    if (this.lastSign !== 0 && sign !== this.lastSign) this.reversals.push(timeMs);
    this.lastSign = sign;
    this.reversals = this.reversals.filter((t) => timeMs - t <= this.windowMs);
    return this.reversals.length >= this.needed;
  }

  reset(): void {
    this.lastSign = 0;
    this.reversals = [];
  }
}

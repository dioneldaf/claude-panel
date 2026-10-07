import { describe, expect, it } from "vitest";
import { ShakeDetector, Spring } from "./physics";

describe("Spring", () => {
  it("converges to its target and comes to rest", () => {
    const s = new Spring(180, 14);
    s.target = 10;
    for (let i = 0; i < 240; i++) s.step(1 / 60);
    expect(s.value).toBeCloseTo(10, 2);
    expect(s.atRest()).toBe(true);
  });

  it("overshoots when underdamped", () => {
    const s = new Spring(200, 6);
    s.target = 1;
    let max = 0;
    for (let i = 0; i < 120; i++) {
      s.step(1 / 60);
      max = Math.max(max, s.value);
    }
    expect(max).toBeGreaterThan(1.05);
  });

  it("keeps its velocity when retargeted mid-flight", () => {
    const s = new Spring(120, 12);
    s.target = 10;
    for (let i = 0; i < 6; i++) s.step(1 / 60);
    const before = s.velocity;
    s.target = -10;
    expect(s.velocity).toBe(before);
    s.step(1 / 60);
    expect(s.value).toBeGreaterThan(0);
  });

  it("stays stable with large or irregular time steps", () => {
    const s = new Spring(300, 10);
    s.target = 5;
    for (const dt of [0.5, 0.001, 2, 0.016, 0.2]) s.step(dt);
    for (let i = 0; i < 300; i++) s.step(1 / 60);
    expect(Number.isFinite(s.value)).toBe(true);
    expect(s.value).toBeCloseTo(5, 1);
  });

  it("responds to an impulse and returns to rest", () => {
    const s = new Spring(200, 16);
    s.impulse(5);
    s.step(1 / 60);
    expect(s.value).toBeGreaterThan(0);
    expect(s.atRest()).toBe(false);
    for (let i = 0; i < 300; i++) s.step(1 / 60);
    expect(s.atRest()).toBe(true);
  });
});

describe("ShakeDetector", () => {
  it("ignores a steady drag in one direction", () => {
    const d = new ShakeDetector();
    let dizzy = false;
    for (let t = 0; t < 2000; t += 16) dizzy = d.push(1.5, t) || dizzy;
    expect(dizzy).toBe(false);
  });

  it("detects quick back-and-forth movement", () => {
    const d = new ShakeDetector();
    let dizzy = false;
    for (let i = 0; i < 12; i++) dizzy = d.push(i % 2 === 0 ? 2 : -2, i * 70) || dizzy;
    expect(dizzy).toBe(true);
  });

  it("ignores slow wiggles and reversals spread over a long time", () => {
    const slow = new ShakeDetector();
    let dizzy = false;
    for (let i = 0; i < 12; i++) dizzy = slow.push(i % 2 === 0 ? 0.1 : -0.1, i * 70) || dizzy;
    expect(dizzy).toBe(false);

    const sparse = new ShakeDetector();
    for (let i = 0; i < 12; i++) dizzy = sparse.push(i % 2 === 0 ? 2 : -2, i * 600) || dizzy;
    expect(dizzy).toBe(false);
  });

  it("can be reset", () => {
    const d = new ShakeDetector();
    for (let i = 0; i < 3; i++) d.push(i % 2 === 0 ? 2 : -2, i * 70);
    d.reset();
    expect(d.push(2, 300)).toBe(false);
    expect(d.push(-2, 370)).toBe(false);
  });
});

import { describe, expect, it } from "vitest";
import { draw, frameFor, H, W, type PixelSurface, type Pose } from "./mascot";

const POSES: Pose[] = [
  "idle",
  "working",
  "alert",
  "sleeping",
  "dragged",
  "dizzy",
  "landing",
  "peek",
  "wave",
  "pull",
  "carry",
];
const BODY = "#d97757";
const DARK = "#2b1b17";

/** Records fills into a grid and reports anything drawn outside the canvas. */
function render(pose: Pose, tick: number): { grid: string[][]; outside: number } {
  const grid = Array.from({ length: H }, () => Array<string>(W).fill(""));
  let outside = 0;
  const surface: PixelSurface = {
    fillStyle: "",
    clearRect: () => {},
    fillRect(x, y, w, h) {
      for (let yy = y; yy < y + h; yy++) {
        for (let xx = x; xx < x + w; xx++) {
          if (xx < 0 || yy < 0 || xx >= W || yy >= H) outside++;
          else grid[yy][xx] = String(this.fillStyle);
        }
      }
    },
  };
  draw(surface, frameFor(pose, tick));
  return { grid, outside };
}

const count = (grid: string[][], color: string) => grid.flat().filter((c) => c === color).length;

describe("mascot frames", () => {
  it("never draws outside the canvas in any pose or frame", () => {
    for (const pose of POSES) {
      for (let tick = 0; tick < 48; tick++) {
        expect(render(pose, tick).outside, `${pose} tick ${tick}`).toBe(0);
      }
    }
  });

  it("always shows a body and two eyes", () => {
    for (const pose of POSES) {
      for (let tick = 0; tick < 24; tick++) {
        const { grid } = render(pose, tick);
        expect(count(grid, BODY), `${pose} tick ${tick}`).toBeGreaterThan(40);
        expect(count(grid, DARK), `${pose} tick ${tick}`).toBeGreaterThanOrEqual(2);
      }
    }
  });

  it("animates every pose", () => {
    for (const pose of POSES) {
      const frames = new Set<string>();
      for (let tick = 0; tick < 24; tick++) frames.add(JSON.stringify(render(pose, tick).grid));
      expect(frames.size, pose).toBeGreaterThan(1);
    }
  });

  it("keeps the feet on the ground except when jumping or held", () => {
    const feetRow = (pose: Pose, tick: number) => render(pose, tick).grid[13].filter((c) => c === BODY).length;
    for (let tick = 0; tick < 12; tick++) {
      expect(feetRow("idle", tick)).toBe(4);
      // While typing, an arm may reach down to keyboard level.
      expect(feetRow("working", tick)).toBeGreaterThanOrEqual(4);
      expect(feetRow("sleeping", tick)).toBe(4);
    }
    expect(feetRow("alert", 2)).toBe(0);
  });
});

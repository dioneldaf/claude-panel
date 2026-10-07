import { describe, expect, it } from "vitest";
import { GLYPH_HEIGHT, measure, pathData, runs } from "./pixelfont";

describe("pixel font", () => {
  it("measures text with one empty column between glyphs", () => {
    expect(measure("")).toBe(0);
    expect(measure("I")).toBe(3);
    expect(measure("II")).toBe(7);
    expect(measure("M")).toBe(5);
    expect(measure("A B")).toBe(3 + 1 + 2 + 1 + 3);
  });

  it("is case-insensitive and renders unknown characters as blanks", () => {
    expect(runs("a")).toEqual(runs("A"));
    expect(runs("é")).toEqual([]);
    expect(measure("é")).toBe(3);
  });

  it("merges horizontal neighbours into runs inside the glyph box", () => {
    const t = runs("T");
    expect(t[0]).toEqual([0, 0, 3]);
    expect(t.slice(1)).toEqual([
      [1, 1, 1],
      [1, 2, 1],
      [1, 3, 1],
      [1, 4, 1],
    ]);
    for (const [x, y, w] of runs("CLAUDE PANEL 0123456789")) {
      expect(y).toBeGreaterThanOrEqual(0);
      expect(y).toBeLessThan(GLYPH_HEIGHT);
      expect(x).toBeGreaterThanOrEqual(0);
      expect(w).toBeGreaterThan(0);
    }
  });

  it("covers the characters used by the interface", () => {
    for (const ch of "ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789") {
      expect(runs(ch).length, ch).toBeGreaterThan(0);
    }
  });

  it("builds an SVG path of unit-high rectangles", () => {
    expect(pathData("I")).toBe("M0 0h3v1h-3zM1 1h1v1h-1zM1 2h1v1h-1zM1 3h1v1h-1zM0 4h3v1h-3z");
  });
});

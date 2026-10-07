// Hand-drawn 5-row pixel font, rendered as crisp SVG rectangles.
// No font files are downloaded; body text falls back to the system monospace font.

export const GLYPH_HEIGHT = 5;

const G: Record<string, string[]> = {
  A: ["010", "101", "111", "101", "101"],
  B: ["110", "101", "110", "101", "110"],
  C: ["011", "100", "100", "100", "011"],
  D: ["110", "101", "101", "101", "110"],
  E: ["111", "100", "110", "100", "111"],
  F: ["111", "100", "110", "100", "100"],
  G: ["011", "100", "101", "101", "011"],
  H: ["101", "101", "111", "101", "101"],
  I: ["111", "010", "010", "010", "111"],
  J: ["001", "001", "001", "101", "010"],
  K: ["101", "101", "110", "101", "101"],
  L: ["100", "100", "100", "100", "111"],
  M: ["10001", "11011", "10101", "10001", "10001"],
  N: ["1001", "1101", "1011", "1001", "1001"],
  O: ["010", "101", "101", "101", "010"],
  P: ["110", "101", "110", "100", "100"],
  Q: ["010", "101", "101", "110", "011"],
  R: ["110", "101", "110", "101", "101"],
  S: ["011", "100", "010", "001", "110"],
  T: ["111", "010", "010", "010", "010"],
  U: ["101", "101", "101", "101", "111"],
  V: ["101", "101", "101", "101", "010"],
  W: ["10001", "10001", "10101", "11011", "10001"],
  X: ["101", "101", "010", "101", "101"],
  Y: ["101", "101", "010", "010", "010"],
  Z: ["111", "001", "010", "100", "111"],
  "0": ["111", "101", "101", "101", "111"],
  "1": ["010", "110", "010", "010", "111"],
  "2": ["110", "001", "010", "100", "111"],
  "3": ["110", "001", "010", "001", "110"],
  "4": ["101", "101", "111", "001", "001"],
  "5": ["111", "100", "110", "001", "110"],
  "6": ["011", "100", "111", "101", "111"],
  "7": ["111", "001", "010", "010", "010"],
  "8": ["111", "101", "111", "101", "111"],
  "9": ["111", "101", "111", "001", "110"],
  " ": ["00", "00", "00", "00", "00"],
  ".": ["0", "0", "0", "0", "1"],
  ":": ["0", "1", "0", "1", "0"],
  "!": ["1", "1", "1", "0", "1"],
  "-": ["000", "000", "111", "000", "000"],
  "+": ["000", "010", "111", "010", "000"],
  "/": ["001", "001", "010", "100", "100"],
  "?": ["110", "001", "010", "000", "010"],
};

const BLANK = ["000", "000", "000", "000", "000"];

function glyph(ch: string): string[] {
  return G[ch.toUpperCase()] ?? BLANK;
}

/** Width of the text in cells, with one empty column between glyphs. */
export function measure(text: string): number {
  const chars = [...text];
  if (chars.length === 0) return 0;
  return chars.reduce((sum, ch) => sum + glyph(ch)[0].length, 0) + chars.length - 1;
}

/** Lit cells merged into horizontal runs: [x, y, width], row by row per glyph. */
export function runs(text: string): Array<[number, number, number]> {
  const out: Array<[number, number, number]> = [];
  let offset = 0;
  for (const ch of text) {
    const rows = glyph(ch);
    rows.forEach((row, y) => {
      let x = 0;
      while (x < row.length) {
        if (row[x] !== "1") {
          x++;
          continue;
        }
        let end = x;
        while (end < row.length && row[end] === "1") end++;
        out.push([offset + x, y, end - x]);
        x = end;
      }
    });
    offset += rows[0].length + 1;
  }
  return out;
}

export function pathData(text: string): string {
  return runs(text)
    .map(([x, y, w]) => `M${x} ${y}h${w}v1h-${w}z`)
    .join("");
}

const SVG_NS = "http://www.w3.org/2000/svg";

/** Builds an SVG element drawing `text`; the colour follows CSS `color`. */
export function pixelText(text: string, scale = 2): SVGSVGElement {
  const width = Math.max(1, measure(text));
  const svg = document.createElementNS(SVG_NS, "svg");
  svg.setAttribute("viewBox", `0 0 ${width} ${GLYPH_HEIGHT}`);
  svg.setAttribute("width", String(width * scale));
  svg.setAttribute("height", String(GLYPH_HEIGHT * scale));
  svg.setAttribute("shape-rendering", "crispEdges");
  svg.setAttribute("aria-hidden", "true");
  svg.classList.add("pixel-text");
  const path = document.createElementNS(SVG_NS, "path");
  path.setAttribute("d", pathData(text));
  path.setAttribute("fill", "currentColor");
  svg.appendChild(path);
  return svg;
}

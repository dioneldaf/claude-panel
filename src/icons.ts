// Pixel icons drawn as bitmaps; rendered as crisp SVG like the font.

const SVG_NS = "http://www.w3.org/2000/svg";

const BITMAPS = {
  bell: ["0001000", "0011100", "0111110", "0111110", "0111110", "1111111", "0001000"],
  slash: ["0000001", "0000010", "0000100", "0001000", "0010000", "0100000", "1000000"],
  mini: ["0000000", "0000000", "0000000", "0000000", "0111110", "0111110", "0000000"],
  hide: ["1000001", "0100010", "0010100", "0001000", "0010100", "0100010", "1000001"],
  plug: ["0100100", "0100100", "1111111", "1111111", "0111110", "0011100", "0001000"],
  help: ["0111110", "1100011", "0000110", "0001100", "0001100", "0000000", "0001100"],
} as const;

export type IconName = keyof typeof BITMAPS;

export function icon(name: IconName, scale = 2): SVGSVGElement {
  const rows = BITMAPS[name];
  const size = rows.length;
  let d = "";
  rows.forEach((row, y) => {
    for (let x = 0; x < row.length; x++) {
      if (row[x] === "1") d += `M${x} ${y}h1v1h-1z`;
    }
  });
  const svg = document.createElementNS(SVG_NS, "svg");
  svg.setAttribute("viewBox", `0 0 ${size} ${size}`);
  svg.setAttribute("width", String(size * scale));
  svg.setAttribute("height", String(size * scale));
  svg.setAttribute("shape-rendering", "crispEdges");
  svg.setAttribute("aria-hidden", "true");
  svg.classList.add("icon");
  const path = document.createElementNS(SVG_NS, "path");
  path.setAttribute("d", d);
  path.setAttribute("fill", "currentColor");
  svg.appendChild(path);
  return svg;
}

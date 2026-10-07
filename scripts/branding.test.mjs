import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { MASCOT_NAME, PRODUCT_NAME } from "../src/branding.ts";

const read = (path) => readFileSync(new URL(`../${path}`, import.meta.url), "utf8");

describe("branding", () => {
  it("uses one product name in the frontend, the Rust core, the window and the page", () => {
    const tauri = JSON.parse(read("src-tauri/tauri.conf.json"));
    expect(tauri.productName).toBe(PRODUCT_NAME);
    expect(tauri.app.windows[0].title).toBe(PRODUCT_NAME);
    expect(read("crates/cpanel-core/src/lib.rs")).toContain(`pub const PRODUCT_NAME: &str = "${PRODUCT_NAME}";`);
    expect(read("index.html")).toContain(`<title>${PRODUCT_NAME}</title>`);
  });

  it("does not hard-code the names anywhere else in the frontend sources", () => {
    for (const file of ["src/main.ts", "src/mascot.ts", "src/drag.ts", "src/model.ts", "src/backend.ts"]) {
      const source = read(file);
      expect(source, file).not.toContain(PRODUCT_NAME);
      expect(source.toUpperCase(), file).not.toContain(PRODUCT_NAME.toUpperCase());
      expect(source, file).not.toContain(MASCOT_NAME);
    }
  });
});

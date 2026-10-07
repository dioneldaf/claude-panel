import { describe, expect, it } from "vitest";
import { cargoWorkspaceVersion, readVersions, versionProblems } from "./check-version.mjs";

const same = { "package.json": "0.1.0", "Cargo.toml": "0.1.0", "src-tauri/tauri.conf.json": "0.1.0" };

describe("versionProblems", () => {
  it("accepts matching versions with or without a tag", () => {
    expect(versionProblems(same)).toEqual([]);
    expect(versionProblems(same, "v0.1.0")).toEqual([]);
  });

  it("reports files that disagree with each other", () => {
    const problems = versionProblems({ ...same, "Cargo.toml": "0.2.0" });
    expect(problems).toHaveLength(1);
    expect(problems[0]).toContain("Cargo.toml");
  });

  it("reports a tag that does not match, including a missing v prefix", () => {
    expect(versionProblems(same, "v0.2.0")[0]).toContain("v0.2.0");
    expect(versionProblems(same, "0.1.0")).toHaveLength(1);
    expect(versionProblems(same, "v0.1.0-rc1")).toHaveLength(1);
  });

  it("reports a version that could not be read", () => {
    expect(versionProblems({ ...same, "Cargo.toml": undefined })[0]).toContain("Cargo.toml");
  });
});

describe("cargoWorkspaceVersion", () => {
  it("reads the version of [workspace.package] only", () => {
    const toml = '[workspace]\nmembers = []\n\n[workspace.package]\nedition = "2021"\nversion = "1.2.3"\n\n[workspace.dependencies]\nserde = { version = "1" }\n';
    expect(cargoWorkspaceVersion(toml)).toBe("1.2.3");
    expect(cargoWorkspaceVersion('[package]\nversion = "9.9.9"\n')).toBeUndefined();
  });
});

describe("this repository", () => {
  it("has one consistent version", () => {
    const versions = readVersions(new URL("..", import.meta.url));
    expect(Object.keys(versions)).toHaveLength(3);
    expect(versionProblems(versions)).toEqual([]);
  });
});

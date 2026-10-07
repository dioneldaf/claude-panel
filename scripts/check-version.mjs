// Checks that package.json, Cargo.toml and tauri.conf.json carry the same version,
// and optionally that a release tag (vX.Y.Z) matches it.
// Usage: node scripts/check-version.mjs [tag]
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

/** Version declared under `[workspace.package]` in a Cargo manifest. */
export function cargoWorkspaceVersion(toml) {
  const section = toml.split(/^\[/m).find((part) => part.startsWith("workspace.package]"));
  return section?.match(/^version\s*=\s*"([^"]+)"/m)?.[1];
}

/** Reads the three version declarations relative to the repository root URL. */
export function readVersions(root) {
  const read = (path) => readFileSync(new URL(path, root), "utf8");
  return {
    "package.json": JSON.parse(read("package.json")).version,
    "Cargo.toml": cargoWorkspaceVersion(read("Cargo.toml")),
    "src-tauri/tauri.conf.json": JSON.parse(read("src-tauri/tauri.conf.json")).version,
  };
}

/** Returns human-readable problems; an empty list means everything matches. */
export function versionProblems(versions, tag) {
  const problems = [];
  const expected = versions["package.json"];
  for (const [file, version] of Object.entries(versions)) {
    if (!version) problems.push(`${file}: version not found`);
    else if (version !== expected) problems.push(`${file}: ${version} differs from package.json ${expected}`);
  }
  if (tag !== undefined && tag !== `v${expected}`) {
    problems.push(`tag ${tag} does not match version ${expected} (expected v${expected})`);
  }
  return problems;
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const versions = readVersions(new URL("..", import.meta.url));
  const problems = versionProblems(versions, process.argv[2]);
  if (problems.length > 0) {
    console.error(problems.join("\n"));
    process.exit(1);
  }
  console.log(`version ${versions["package.json"]} is consistent${process.argv[2] ? ` with tag ${process.argv[2]}` : ""}`);
}

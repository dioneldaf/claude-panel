// Builds the distributable artefacts into release/:
//   claude-panel-<version>-windows-x64-setup.exe      NSIS installer (per user)
//   claude-panel-<version>-windows-x64-portable.zip   both executables, LICENSE, README.txt
//   SHA256SUMS                                        checksums of the two files above
//
// Usage: node scripts/build-release.mjs [--no-bundle]
//   --no-bundle   only build the two executables in target/release (no installer, no zip)
import { execSync } from "node:child_process";
import { createHash } from "node:crypto";
import { copyFileSync, mkdirSync, readdirSync, readFileSync, rmSync, statSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { readVersions, versionProblems } from "./check-version.mjs";
import { createZip } from "./zip.mjs";

const rootUrl = new URL("..", import.meta.url);
const root = fileURLToPath(rootUrl);
const run = (command) => execSync(command, { cwd: root, stdio: "inherit" });

const versions = readVersions(rootUrl);
const problems = versionProblems(versions);
if (problems.length > 0) {
  console.error(problems.join("\n"));
  process.exit(1);
}
const version = versions["package.json"];
const noBundle = process.argv.includes("--no-bundle");

const releaseDir = join(root, "target", "release");
const appExe = join(releaseDir, "claude-panel.exe");
const hookExe = join(releaseDir, "cpanel-hook.exe");

// 1. The hook is built first: the installer ships it as a resource next to the app.
run("cargo build --release -p cpanel-hook");
const staged = join(root, "src-tauri", "bin");
mkdirSync(staged, { recursive: true });
copyFileSync(hookExe, join(staged, "cpanel-hook.exe"));

// 2. Application, and the installer unless --no-bundle.
if (noBundle) {
  run("pnpm tauri build --no-bundle");
  console.log(`\nBuilt ${appExe}\n      ${hookExe}`);
  process.exit(0);
}
const nsisDir = join(releaseDir, "bundle", "nsis");
rmSync(nsisDir, { recursive: true, force: true });
run("pnpm tauri build --config src-tauri/tauri.bundle.conf.json");

// 3. Collect artefacts under stable, lower-case names.
const out = join(root, "release");
rmSync(out, { recursive: true, force: true });
mkdirSync(out, { recursive: true });
const base = `claude-panel-${version}-windows-x64`;

const installers = readdirSync(nsisDir).filter((name) => name.endsWith("-setup.exe"));
if (installers.length !== 1) {
  console.error(`expected exactly one installer in ${nsisDir}, found: ${installers.join(", ") || "none"}`);
  process.exit(1);
}
const installerName = `${base}-setup.exe`;
copyFileSync(join(nsisDir, installers[0]), join(out, installerName));

// 4. Portable archive.
const zipName = `${base}-portable.zip`;
const zip = createZip(
  [
    { name: "claude-panel.exe", data: readFileSync(appExe) },
    { name: "cpanel-hook.exe", data: readFileSync(hookExe) },
    { name: "LICENSE.txt", data: readFileSync(join(root, "LICENSE")) },
    { name: "README.txt", data: readFileSync(join(root, "packaging", "portable-readme.txt")) },
  ],
  new Date(),
);
writeFileSync(join(out, zipName), zip);

// 5. Checksums, in the format `sha256sum -c` and `Get-FileHash` users expect.
const sums = [installerName, zipName]
  .map((name) => `${createHash("sha256").update(readFileSync(join(out, name))).digest("hex")}  ${name}`)
  .join("\n");
writeFileSync(join(out, "SHA256SUMS"), `${sums}\n`);

console.log("\nRelease artefacts:");
for (const name of [installerName, zipName, "SHA256SUMS"]) {
  console.log(`  ${join(out, name)}  (${statSync(join(out, name)).size} bytes)`);
}
console.log(`\n${sums}`);

import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

const root = new URL("../../", import.meta.url);
const read = (path) => readFileSync(new URL(path, root), "utf8");
const json = (path) => JSON.parse(read(path));
const desktop = json("apps/desktop/package.json");
const lock = json("apps/desktop/package-lock.json");
const tauri = json("apps/desktop/src-tauri/tauri.conf.json");
const version = desktop.version;

assert.match(version, /^\d+\.\d+\.\d+$/, "desktop version must be stable SemVer");
assert.equal(
  read(`.github/release-notes/${version}.md`).split(/\r?\n/, 1)[0],
  `# Cursor BYOK v${version}`,
  "release notes must exist and match the desktop version",
);
for (const [name, actual] of Object.entries({
  "package-lock.json": lock.version,
  "package-lock.json root package": lock.packages[""].version,
  "tauri.conf.json": tauri.version,
  "src-tauri/Cargo.toml": read("apps/desktop/src-tauri/Cargo.toml").match(/^version = "([^"]+)"/m)?.[1],
  "Cargo.lock desktop package": read("Cargo.lock").match(/name = "cursor-byok-desktop"\nversion = "([^"]+)"/)?.[1],
})) {
  assert.equal(actual, version, `${name} version must match ${version}`);
}

const repository = desktop.repository.url
  .replace(/^git\+https:\/\/github\.com\//, "")
  .replace(/\.git$/, "");
assert.match(repository, /^[\w.-]+\/[\w.-]+$/, "invalid project repository");
const releases = `https://github.com/${repository}/releases/latest/download`;
assert.deepEqual(tauri.plugins.updater.endpoints, [`${releases}/latest.json`]);
assert.ok(
  read("apps/desktop/src-tauri/src/update/mod.rs").includes(`"${releases}/portable-latest.json"`),
  "Windows updater must use the project repository",
);
const publicKey = Buffer.from(tauri.plugins.updater.pubkey, "base64").toString("utf8");
assert.match(publicKey, /^untrusted comment: .+\n[\w+/=]+\s*$/, "invalid updater public key");
for (const name of ["LICENSE", "THIRD-PARTY-LICENSES.md"]) {
  assert.equal(tauri.bundle.resources[`../../../${name}`], `licenses/${name}`);
  assert.ok(read(name).includes("MIT License"), `${name} must retain the MIT license`);
}
for (const name of ["agent_v1.proto", "aiserver_v1.proto"]) {
  assert.ok(read(`protocols/cursor/${name}`).length > 0, `${name} is required`);
}

const args = process.argv.slice(2);
if (args.length) {
  assert.equal(args.length, 2, "usage: check-release.mjs [--tag vMAJOR.MINOR.PATCH]");
  assert.equal(args[0], "--tag");
  assert.equal(args[1], `v${version}`, "release tag must match the desktop version");
}
console.log(`Release configuration verified: ${repository} v${version} (${fileURLToPath(root)})`);

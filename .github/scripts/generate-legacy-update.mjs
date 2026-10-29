import { createHash } from "node:crypto";
import { stat, readFile, writeFile } from "node:fs/promises";
import { basename, join, resolve } from "node:path";

const assetSpecs = [
  ["macos-arm64", ".tar.gz"],
  ["macos-amd64", ".tar.gz"],
  ["windows-amd64", ".zip"],
  ["linux-amd64", ".tar.gz"],
];

function readOptions(args) {
  const options = new Map();
  for (let index = 0; index < args.length; index += 2) {
    const name = args[index];
    const value = args[index + 1];
    if (!name?.startsWith("--") || value === undefined) {
      throw new Error(`invalid argument near ${name ?? "end of command"}`);
    }
    options.set(name.slice(2), value);
  }
  return options;
}

function required(options, name) {
  const value = options.get(name)?.trim();
  if (!value) throw new Error(`--${name} is required`);
  return value;
}

async function sha256(path) {
  const content = await readFile(path);
  return createHash("sha256").update(content).digest("hex");
}

async function main() {
  const options = readOptions(process.argv.slice(2));
  const version = required(options, "version").replace(/^v/, "");
  const repository = required(options, "repository");
  const assetsDir = resolve(required(options, "assets-dir"));
  const output = resolve(required(options, "output"));
  const releaseNotes = options.get("notes")?.trim() || `Cursor BYOK v${version}`;

  if (!/^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?$/.test(version)) {
    throw new Error(`invalid semantic version: ${version}`);
  }
  if (!/^[^/\s]+\/[^/\s]+$/.test(repository)) {
    throw new Error(`invalid GitHub repository: ${repository}`);
  }

  const platforms = {};
  for (const [platform, suffix] of assetSpecs) {
    const filename = `cursor-byok-${version}-${platform}${suffix}`;
    const path = join(assetsDir, filename);
    const info = await stat(path);
    if (!info.isFile()) throw new Error(`release asset is not a file: ${path}`);
    platforms[platform] = {
      url: `https://github.com/${repository}/releases/download/v${version}/${basename(path)}`,
      size: info.size,
      checksum: `sha256:${await sha256(path)}`,
    };
  }

  const manifest = {
    version,
    release_date: new Date().toISOString(),
    release_notes: releaseNotes,
    platforms,
    mandatory: false,
  };
  await writeFile(output, `${JSON.stringify(manifest, null, 2)}\n`);
}

main().catch((error) => {
  console.error(error instanceof Error ? error.message : String(error));
  process.exitCode = 1;
});

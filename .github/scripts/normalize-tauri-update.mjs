import { readFile, writeFile } from "node:fs/promises";
import { resolve } from "node:path";
import { pathToFileURL } from "node:url";

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

export function normalizeTauriUpdate(manifest, release, repository, version) {
  if (manifest.version !== version) {
    throw new Error(
      `updater manifest version ${manifest.version ?? "is missing"}; expected ${version}`,
    );
  }
  if (release.tag_name !== `v${version}`) {
    throw new Error(
      `GitHub release tag ${release.tag_name ?? "is missing"}; expected v${version}`,
    );
  }
  if (!manifest.platforms || typeof manifest.platforms !== "object") {
    throw new Error("updater manifest has no platforms");
  }

  const assetsBySourceUrl = new Map();
  const publicAssetUrls = new Set();
  for (const asset of release.assets ?? []) {
    if (!asset?.id || !asset?.name) continue;
    const publicUrl = `https://github.com/${repository}/releases/download/v${version}/${encodeURIComponent(asset.name)}`;
    assetsBySourceUrl.set(
      `https://api.github.com/repos/${repository}/releases/assets/${asset.id}`,
      publicUrl,
    );
    if (asset.browser_download_url) {
      assetsBySourceUrl.set(asset.browser_download_url, publicUrl);
    }
    publicAssetUrls.add(publicUrl);
  }

  for (const [platform, entry] of Object.entries(manifest.platforms)) {
    if (!entry?.signature || !entry?.url) {
      throw new Error(`updater platform ${platform} is missing its URL or signature`);
    }
    const publicUrl = assetsBySourceUrl.get(entry.url) ?? entry.url;
    if (!publicAssetUrls.has(publicUrl)) {
      throw new Error(`updater platform ${platform} references an unknown release asset`);
    }
    entry.url = publicUrl;
  }

  return manifest;
}

async function main() {
  const options = readOptions(process.argv.slice(2));
  const manifestPath = resolve(required(options, "manifest"));
  const releasePath = resolve(required(options, "release"));
  const repository = required(options, "repository");
  const version = required(options, "version").replace(/^v/, "");

  if (!/^[^/\s]+\/[^/\s]+$/.test(repository)) {
    throw new Error(`invalid GitHub repository: ${repository}`);
  }
  if (!/^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?$/.test(version)) {
    throw new Error(`invalid semantic version: ${version}`);
  }

  const manifest = JSON.parse(await readFile(manifestPath, "utf8"));
  const release = JSON.parse(await readFile(releasePath, "utf8"));
  const normalized = normalizeTauriUpdate(
    manifest,
    release,
    repository,
    version,
  );
  await writeFile(manifestPath, `${JSON.stringify(normalized, null, 2)}\n`);
}

if (
  process.argv[1] &&
  import.meta.url === pathToFileURL(resolve(process.argv[1])).href
) {
  main().catch((error) => {
    console.error(error instanceof Error ? error.message : String(error));
    process.exitCode = 1;
  });
}

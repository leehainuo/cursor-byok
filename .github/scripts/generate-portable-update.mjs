import { readFile, writeFile } from "node:fs/promises";
import { basename, resolve } from "node:path";
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

export function generatePortableUpdate({ version, repository, assetName, signature }) {
  const normalizedVersion = version.replace(/^v/, "");
  if (!/^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?$/.test(normalizedVersion)) {
    throw new Error(`invalid semantic version: ${normalizedVersion}`);
  }
  if (!/^[^/\s]+\/[^/\s]+$/.test(repository)) {
    throw new Error(`invalid GitHub repository: ${repository}`);
  }
  if (!assetName || basename(assetName) !== assetName) {
    throw new Error("asset name must be a file name");
  }
  if (!signature.trim()) throw new Error("signature is required");

  return {
    version: normalizedVersion,
    notes: `Cursor BYOK v${normalizedVersion}`,
    pub_date: new Date().toISOString(),
    platforms: {
      "windows-x86_64": {
        signature: signature.trim(),
        url: `https://github.com/${repository}/releases/download/v${normalizedVersion}/${encodeURIComponent(assetName)}`,
      },
    },
  };
}

async function main() {
  const options = readOptions(process.argv.slice(2));
  const version = required(options, "version");
  const repository = required(options, "repository");
  const asset = required(options, "asset");
  const signaturePath = resolve(required(options, "signature"));
  const output = resolve(required(options, "output"));
  const manifest = generatePortableUpdate({
    version,
    repository,
    assetName: basename(asset),
    signature: await readFile(signaturePath, "utf8"),
  });
  await writeFile(output, `${JSON.stringify(manifest, null, 2)}\n`);
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  main().catch((error) => {
    console.error(error instanceof Error ? error.message : String(error));
    process.exitCode = 1;
  });
}

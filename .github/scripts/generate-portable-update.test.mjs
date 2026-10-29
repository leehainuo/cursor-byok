import assert from "node:assert/strict";
import test from "node:test";
import { generatePortableUpdate } from "./generate-portable-update.mjs";

test("generates a signed Windows portable updater manifest", () => {
  const manifest = generatePortableUpdate({
    version: "v1.2.3-beta.1",
    repository: "owner/repository",
    assetName: "cursor-byok-1.2.3-beta.1-windows-amd64.zip",
    signature: "signed-payload\n",
  });

  assert.equal(manifest.version, "1.2.3-beta.1");
  assert.deepEqual(Object.keys(manifest.platforms), ["windows-x86_64"]);
  assert.equal(manifest.platforms["windows-x86_64"].signature, "signed-payload");
  assert.equal(
    manifest.platforms["windows-x86_64"].url,
    "https://github.com/owner/repository/releases/download/v1.2.3-beta.1/cursor-byok-1.2.3-beta.1-windows-amd64.zip",
  );
});

test("rejects invalid inputs", () => {
  assert.throws(() => generatePortableUpdate({
    version: "latest",
    repository: "owner/repository",
    assetName: "update.zip",
    signature: "signature",
  }), /semantic version/);
  assert.throws(() => generatePortableUpdate({
    version: "1.2.3",
    repository: "owner/repository",
    assetName: "../update.zip",
    signature: "signature",
  }), /file name/);
  assert.throws(() => generatePortableUpdate({
    version: "1.2.3",
    repository: "owner/repository",
    assetName: "update.zip",
    signature: " ",
  }), /signature/);
});

import assert from "node:assert/strict";
import test from "node:test";
import { normalizeTauriUpdate } from "./normalize-tauri-update.mjs";

function releaseFixture() {
  const platforms = ["darwin-aarch64", "darwin-x86_64", "linux-x86_64", "windows-x86_64"];
  const assets = platforms.map((platform, index) => ({ id: index + 1, name: `${platform}.zip` }));
  const manifest = {
    version: "1.0.0",
    platforms: Object.fromEntries(platforms.map((platform, index) => [platform, {
      signature: `signature-${platform}`,
      url: `https://api.github.com/repos/owner/repository/releases/assets/${index + 1}`,
    }])),
  };
  return { manifest, release: { tag_name: "v1.0.0", assets } };
}

test("converts private API asset URLs to public downloads while retaining signatures", () => {
  const { manifest, release } = releaseFixture();
  const result = normalizeTauriUpdate(manifest, release, "owner/repository", "1.0.0");
  for (const [platform, entry] of Object.entries(result.platforms)) {
    assert.equal(entry.url, `https://github.com/owner/repository/releases/download/v1.0.0/${platform}.zip`);
    assert.equal(entry.signature, `signature-${platform}`);
  }
});

test("refuses an incomplete multi-platform release", () => {
  const { manifest, release } = releaseFixture();
  delete manifest.platforms["darwin-x86_64"];
  assert.throws(() => normalizeTauriUpdate(manifest, release, "owner/repository", "1.0.0"), /missing release platform/);
});

test("refuses an asset from outside the release", () => {
  const { manifest, release } = releaseFixture();
  manifest.platforms["windows-x86_64"].url = "https://example.com/untrusted.zip";
  assert.throws(() => normalizeTauriUpdate(manifest, release, "owner/repository", "1.0.0"), /unknown release asset/);
});

test("refuses unsigned artifacts and mismatched release versions", () => {
  const { manifest, release } = releaseFixture();
  assert.throws(() => normalizeTauriUpdate(manifest, release, "owner/repository", "1.0.1"), /expected 1.0.1/);
  manifest.platforms["linux-x86_64"].signature = "";
  assert.throws(() => normalizeTauriUpdate(manifest, release, "owner/repository", "1.0.0"), /missing its URL or signature/);
});

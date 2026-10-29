---
name: release
description: Prepare, authorize, publish, troubleshoot, and verify Cursor BYOK desktop GitHub Releases. Use for version bumps, release tags, GitHub Actions release runs, updater manifests, signing, or release-readiness checks.
---

# Desktop release

Release through `.github/workflows/release.yml` in `leehainuo/cursor-byok`. The independent release series starts at `1.0.0`.

## Publication authority

- The maintainer, GitHub user `leehainuo`, authorizes releases. Verify the authenticated account before publication.
- Require an explicit instruction to publish before pushing a `v*` tag, rerunning a release workflow, or publishing or editing a GitHub Release. Preparing a commit or configuring the repository does not authorize publication.
- Ordinary pushes to `main` run CI only. Release tags must point to commits contained in `origin/main`.
- Never print or commit `.tauri/cursor-byok.key`. Upload it only to this repository's `TAURI_SIGNING_PRIVATE_KEY` Actions secret when the maintainer authorizes signing-secret configuration. Preserve a secure backup of the key: changing it breaks updates for installed clients.
- Never delete, replace, or move an existing tag or published Release without separate explicit authorization. Diagnose a failed release before requesting another live attempt.

## Release inputs

Keep the desktop version identical in these files; leave the independent server version unchanged:

```text
cursor-byok/
├── Cargo.lock                            # Desktop package lock entry
├── apps/desktop/
│   ├── package.json                      # Frontend package version
│   ├── package-lock.json                 # Frontend lock and root package versions
│   └── src-tauri/
│       ├── Cargo.toml                    # Desktop Rust package version
│       └── tauri.conf.json               # App version, updater endpoint and public key
├── protocols/cursor/
│   ├── agent_v1.proto                    # Agent protocol build input
│   └── aiserver_v1.proto                 # Validated transport wire contracts
└── .github/workflows/release.yml         # Tagged multi-platform build and publication
```

Use stable SemVer tags, `vMAJOR.MINOR.PATCH`. Never republish an existing version. The updater resolves GitHub's `/releases/latest/download/` path, so releases must be normal Latest releases with `prerelease: false`.

## Prepare and validate

1. Inspect local changes and `origin/main`. Preserve unrelated work and confirm the intended tag and Release do not exist.
2. Update the version in the five files above, including both frontend lockfile version fields.
3. Confirm `tauri.conf.json` uses this repository's updater endpoint and the public key matching the private key in `TAURI_SIGNING_PRIVATE_KEY`. The password secret must be absent when the key has no password.
4. Run `make check`, then `npm --prefix apps/desktop run tauri:build -- --debug --no-bundle`. `check-release.mjs` verifies versions, endpoints, licenses, and protocol inputs; the workflow runs CI again before building release artifacts.
5. Validate the workflow and inspect the staged files. Keep `.tauri/` out of Git. Desktop resources and Docker images must include `LICENSE` and `THIRD-PARTY-LICENSES.md`.

## Publish and verify

After publication is explicitly authorized:

1. Push the reviewed commit to `main` and wait for CI to pass.
2. Create and push only its matching version tag. Follow the triggered release run; stop and diagnose on failure.
3. Confirm the published release is Latest and contains signed Tauri updater artifacts and `latest.json` for all four targets: `darwin-aarch64`, `darwin-x86_64`, `linux-x86_64`, and `windows-x86_64`.
4. Confirm the Windows portable ZIP contains `cursor-byok-desktop.exe`, `LICENSE`, and `THIRD-PARTY-LICENSES.md`, with its signature and `portable-latest.json`. The current Windows client uses this signed portable update path.
5. Verify updater URLs refer to public assets in this release. No legacy `update.json` or archives are generated for retired clients.
6. Report the run and Release URLs. Updater signing is distinct from Apple code signing/notarization and Windows code signing; do not claim platform trust that was not configured and verified.

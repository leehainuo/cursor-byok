# Contributing

Issues and pull requests are welcome. Keep each change focused, follow the existing directory structure, and describe the problem and validation results. Use Conventional Commits, such as `fix: correct model selection` or `docs: clarify setup`.

欢迎提交 Issue 和 Pull Request。每次改动保持聚焦，遵循现有目录结构，并说明解决的问题及验证结果。提交信息使用 Conventional Commits。

## Development

Install Rust stable, Node.js 22 or newer, and the [Tauri 2 prerequisites](https://v2.tauri.app/start/prerequisites/) for your platform.

```bash
npm --prefix apps/desktop ci
make dev-desktop
```

Before opening a pull request:

```bash
make check
```

## Releases

Desktop versions must match in `package.json`, `package-lock.json`, `src-tauri/Cargo.toml`, `tauri.conf.json`, and the desktop entry in `Cargo.lock`.

Write the release notes in `.github/release-notes/MAJOR.MINOR.PATCH.md`. The workflow appends the platform download guide before publication.

Run `node .github/scripts/check-release.mjs`, then validate the desktop build with `npm --prefix apps/desktop run tauri:build -- --debug --no-bundle`.

The maintainer publishes releases by pushing a matching `vMAJOR.MINOR.PATCH` tag after CI passes on `main`. The release workflow builds all platforms, verifies the updater manifests, and publishes only after every build succeeds. Ordinary commits do not publish releases.

Updater signing uses the repository's `TAURI_SIGNING_PRIVATE_KEY` Actions secret. Its public key must match `tauri.conf.json`. Keep a secure backup of the private key outside Git; changing it breaks updates for installed clients. These signatures are separate from Apple notarization and Windows code signing.

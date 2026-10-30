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

## Dependency updates

Dependabot checks on Mondays at 09:00 Asia/Shanghai. Ordinary patch updates are grouped; ordinary minor and major updates get individual PRs. React, Babel, Tauri, and Protobuf packages are grouped by family at every version level, including major upgrades. SQLx, similar, json5, and auto-launch stay out of the general Rust patch group. Security updates remain enabled and are not included in version-update groups.

All updates require passing CI and maintainer review. Patch updates need checks of affected behavior; minor updates need runtime regression checks; major updates also need a migration review. React and React DOM must stay aligned, and Protobuf code generation and runtime versions must remain compatible. Tauri's npm and Cargo groups are separate PRs and must be checked together when their requirements change.

For sensitive changes, verify Cursor protocol encoding, database behavior, file diffs, preservation of Cursor settings, or desktop startup/relaunch/update behavior as applicable. Check Node.js requirements when upgrading build tools. A green CI run does not replace these checks, and dependency PRs are not automatically merged.

依赖更新按普通补丁、独立版本升级和关联依赖家族组织。所有更新都需要 CI 和维护者审查；涉及协议、数据库、文件差异、Cursor 配置或桌面生命周期时，需补充对应的运行验证。

## Releases

Desktop versions must match in `package.json`, `package-lock.json`, `src-tauri/Cargo.toml`, `tauri.conf.json`, and the desktop entry in `Cargo.lock`.

Write the release notes in `.github/release-notes/MAJOR.MINOR.PATCH.md`. The workflow appends the platform download guide before publication.

Run `node .github/scripts/check-release.mjs`, then validate the desktop build with `npm --prefix apps/desktop run tauri:build -- --debug --no-bundle`.

The maintainer publishes releases by pushing a matching `vMAJOR.MINOR.PATCH` tag after CI passes on `main`. The release workflow builds all platforms, verifies the updater manifests, and publishes only after every build succeeds. Ordinary commits do not publish releases. To recover an unpublished release after fixing the workflow, run `Release desktop app` manually from `main` with its existing tag. The workflow validates and builds the tag commit without moving it; published versions are rejected.

Updater signing uses the repository's `TAURI_SIGNING_PRIVATE_KEY` Actions secret. Its public key must match `tauri.conf.json`. Keep a secure backup of the private key outside Git; changing it breaks updates for installed clients. These signatures are separate from Apple notarization and Windows code signing.

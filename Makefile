LOCAL_TAURI_SIGNING_KEY := $(CURDIR)/.tauri/cursor-byok.local.key

.PHONY: check dev-web dev-server dev-desktop build-web build-server build-desktop build-docker

check:
	cargo fmt --all -- --check
	cargo clippy --workspace --all-targets -- -D warnings
	cargo test --workspace --all-targets
	npm --prefix apps/desktop run check

dev-web:
	npm --prefix apps/desktop run dev:web

dev-server:
	CURSOR_CONSOLE_DIR=apps/desktop/dist cargo run --package cursor-server --bin cursor-server

dev-desktop:
	npm --prefix apps/desktop run tauri:dev

build-web:
	npm --prefix apps/desktop run build

build-server:
	cargo build --release --package cursor-server --bin cursor-server

ifeq ($(OS),Windows_NT)
$(LOCAL_TAURI_SIGNING_KEY):
	@powershell -NoProfile -Command "New-Item -ItemType Directory -Force -Path '$(dir $@)' | Out-Null; & '$(CURDIR)/apps/desktop/node_modules/.bin/tauri.cmd' signer generate --ci --write-keys '$@'"

build-desktop: $(LOCAL_TAURI_SIGNING_KEY)
	@node -e "const { spawnSync } = require('node:child_process'); const result = spawnSync(process.execPath, ['node_modules/@tauri-apps/cli/tauri.js', 'build', '--bundles', 'nsis'], { cwd: 'apps/desktop', stdio: 'inherit', env: { ...process.env, TAURI_SIGNING_PRIVATE_KEY: process.argv[1], TAURI_SIGNING_PRIVATE_KEY_PASSWORD: '' } }); process.exit(result.status ?? 1)" "$(LOCAL_TAURI_SIGNING_KEY)"
else
$(LOCAL_TAURI_SIGNING_KEY):
	@install -d -m 700 "$(dir $@)"
	@apps/desktop/node_modules/.bin/tauri signer generate --ci --write-keys "$@" >/dev/null
	@chmod 600 "$@" "$@.pub"

build-desktop: $(LOCAL_TAURI_SIGNING_KEY)
	TAURI_SIGNING_PRIVATE_KEY="$(LOCAL_TAURI_SIGNING_KEY)" TAURI_SIGNING_PRIVATE_KEY_PASSWORD="" npm --prefix apps/desktop run tauri:build
endif

build-docker:
	docker build --tag cursor-byok:local .

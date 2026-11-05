.PHONY: test build lint dist dist-cross dist-darwin app-dev app-build app-test app-server app-resources test-remote

API = --manifest-path api/Cargo.toml --target-dir target

test:
	cargo test --offline
	cargo test --offline $(API)

build:
	cargo build --offline --release
	cargo build --offline --release $(API)

lint:
	cargo clippy --offline --all-targets -- -D warnings
	cargo clippy --offline $(API) --all-targets -- -D warnings

dist:
	scripts/dist.sh

APP = pnpm --dir app

app/node_modules:
	$(APP) install --frozen-lockfile

app-dev: app/node_modules
	$(APP) tauri dev

VERSION = $(shell sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
LINUX_SERVERS = linux-x86_64:x86_64-unknown-linux-musl linux-aarch64:aarch64-unknown-linux-musl
DARWIN_SERVERS = darwin-x86_64:x86_64-apple-darwin darwin-aarch64:aarch64-apple-darwin
WINDOWS = x86_64-pc-windows-msvc

dist-darwin:
	mkdir -p dist
	for p in $(DARWIN_SERVERS); do \
	  t=$${p#*:}; \
	  cargo build --locked --release --target $$t && \
	  cp target/$$t/release/overseer dist/overseer-$(VERSION)-$$t || exit 1; \
	done

dist-cross:
	mkdir -p dist
	for p in $(LINUX_SERVERS); do \
	  t=$${p#*:}; \
	  cargo zigbuild --locked --release --target $$t && \
	  cp target/$$t/release/overseer dist/overseer-$(VERSION)-$$t || exit 1; \
	done
	cargo zigbuild --locked --release $(API) --target x86_64-unknown-linux-musl
	cp target/x86_64-unknown-linux-musl/release/overseer-api dist/overseer-api-$(VERSION)-x86_64-unknown-linux-musl
	cargo xwin build --locked --release --target $(WINDOWS)
	cp target/$(WINDOWS)/release/overseer.exe dist/overseer-$(VERSION)-$(WINDOWS).exe

app-resources:
	scripts/app-resources.sh $(VERSION)

app-server: dist
	$(MAKE) app-resources

app-build: app/node_modules app-server
	$(APP) tauri build

REMOTE_BIN = dist/overseer-$(VERSION)-x86_64-unknown-linux-musl

test-remote:
	DIST_TARGETS=x86_64-unknown-linux-musl scripts/dist.sh
	cargo build --offline --bin overseer
	OVERSEER_DOCKER_TESTS=1 OVERSEER_REMOTE_BIN=$(CURDIR)/$(REMOTE_BIN) OVERSEER_BIN=$(CURDIR)/target/debug/overseer \
	  cargo test --manifest-path app/src-tauri/Cargo.toml --lib remote_it -- --nocapture --test-threads=1

app-test: app/node_modules
	$(APP) test:coverage
	$(APP) check
	cargo build --offline --bin overseer
	OVERSEER_BIN=$(CURDIR)/target/debug/overseer cargo test --manifest-path app/src-tauri/Cargo.toml
	cargo clippy --manifest-path app/src-tauri/Cargo.toml --all-targets -- -D warnings

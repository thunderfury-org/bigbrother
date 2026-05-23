WEB_DIR := web

build-web:
	cd $(WEB_DIR) && pnpm install --frozen-lockfile && pnpm build

build: build-web
	cargo build

build-release: build-web
	cargo build -r

clean:
	cargo clean
	rm -rf $(WEB_DIR)/dist $(WEB_DIR)/node_modules

fmt:
	cargo fmt --all

lint:
	cargo fmt --all -- --check
	cargo clippy -- -D warnings

test:
	cargo test

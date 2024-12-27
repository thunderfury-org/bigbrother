
build:
	cargo build

build-release:
	cargo build -r

clean:
	cargo clean

fmt:
	cargo fmt --all

lint:
	cargo fmt --all -- --check
	cargo clippy -- -D warnings

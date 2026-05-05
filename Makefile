.PHONY: build install test lint fmt init-opencode

build:
	cargo build

install:
	cargo install --path . --force

test:
	cargo test

lint:
	cargo clippy -- -D warnings

fmt:
	cargo fmt

## Scaffold .opencode/ with ECC skills, agents, and rules for OpenCode AI
init-opencode: build
	cargo run -- init opencode

.PHONY: backend check

backend:
	cargo build --release
	install -Dm755 target/release/time-tracker bin/time-tracker

check:
	cargo fmt --check
	cargo test
	cargo clippy -- -D warnings
	omarchy plugin validate .
	timeout 2 quickshell --no-color --path Service.qml || test $$? -eq 124

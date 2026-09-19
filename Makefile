.PHONY: backend check template-check

backend:
	cargo build --release
	install -Dm755 target/release/omatracker bin/omatracker

check:
	cargo fmt --check
	cargo test
	cargo clippy -- -D warnings
	$(MAKE) template-check
	omarchy plugin validate .
	timeout 2 quickshell --no-color --path Service.qml || test $$? -eq 124

template-check:
	@if command -v typst >/dev/null; then \
		typst compile --root . tests/validate-detailed.typ target/template-detailed.pdf; \
		typst compile --root . tests/validate-summary.typ target/template-summary.pdf; \
	else \
		printf '%s\n' "Typst unavailable; skipping template compilation"; \
	fi

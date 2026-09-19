.PHONY: backend check template-check qml-check ui-check

backend:
	cargo build --release
	install -Dm755 target/release/omatracker bin/omatracker

check:
	cargo fmt --check
	cargo test
	cargo clippy -- -D warnings
	$(MAKE) template-check
	omarchy plugin validate .
	$(MAKE) qml-check
	$(MAKE) ui-check

ui-check:
	python tests/ui-check.py

qml-check: backend
	@temporary=$$(mktemp -d); \
	trap 'rm -rf "$$temporary"' EXIT; \
	  OMATRACKER_TEST_DIR="$$temporary" QT_QPA_PLATFORM=offscreen \
	  timeout 15 quickshell --no-color --path ServiceTest.qml && \
	  test -f "$$temporary/passed" && \
	  OMATRACKER_TEST_DIR="$$temporary" QT_QPA_PLATFORM=offscreen \
	  timeout 15 quickshell --no-color --path TemplateServiceTest.qml && \
	  test -f "$$temporary/templates-passed" && \
	  HOME="$$temporary" XDG_CONFIG_HOME="$$temporary/config" \
	  OMATRACKER_TEST_DIR="$$temporary" QT_QPA_PLATFORM=offscreen \
	  timeout 15 quickshell --no-color --path RateTest.qml && \
	  test -f "$$temporary/rates-passed"

template-check:
	typst compile --root . tests/validate-detailed.typ target/template-detailed.pdf
	typst compile --root . tests/validate-summary.typ target/template-summary.pdf
	typst compile --root . tests/validate-rates.typ target/template-rates.pdf

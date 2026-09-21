.PHONY: backend install install-bin install-plugin install-plugin-bin install-check check template-check qml-check ui-check

BINDIR ?= $(HOME)/.local/bin
DATADIR ?= $(if $(XDG_DATA_HOME),$(XDG_DATA_HOME),$(HOME)/.local/share)/omatracker
PLUGIN_DIR ?= $(HOME)/.config/omarchy/plugins/epgeroy.omatracker
PLUGIN_BACKUP_DIR ?= $(if $(XDG_STATE_HOME),$(XDG_STATE_HOME),$(HOME)/.local/state)/omatracker/plugin-backups

backend:
	cargo build --release
	install -Dm755 target/release/omatracker bin/omatracker

# Build from source, then install the same layout used by the plugin release.
install: backend
	$(MAKE) install-bin

# Prebuilt releases can use this target without installing Rust.
install-bin:
	test -x bin/omatracker
	install -d "$(BINDIR)" "$(DATADIR)/bin" "$(DATADIR)/templates"
	install -m755 bin/omatracker "$(DATADIR)/bin/omatracker"
	install -m644 templates/*.typ "$(DATADIR)/templates/"
	ln -sfn "$$(realpath "$(DATADIR)")/bin/omatracker" "$(BINDIR)/omatracker"

install-plugin: backend
	$(MAKE) install-plugin-bin

# Keep the widget and CLI on the same backend without resolving a binary from PATH.
install-plugin-bin: install-bin
	python scripts/install-plugin.py --source . --destination "$(PLUGIN_DIR)" \
	  --backend "$(DATADIR)/bin/omatracker" --backup-root "$(PLUGIN_BACKUP_DIR)"

install-check: backend
	python tests/install-check.py

check:
	cargo fmt --check
	cargo test
	cargo clippy --all-targets -- -D warnings
	$(MAKE) template-check
	omarchy plugin validate .
	$(MAKE) qml-check
	$(MAKE) ui-check
	$(MAKE) install-check

ui-check:
	python tests/ui-check.py

qml-check: backend
	python tests/external-refresh-check.py
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
	typst compile --root . tests/validate-literal-footer.typ target/template-literal-footer.pdf
	typst compile --root . tests/validate-invoice.typ target/template-invoice.pdf
	typst compile --root . tests/validate-detailed.typ target/template-detailed.pdf
	typst compile --root . tests/validate-summary.typ target/template-summary.pdf
	typst compile --root . tests/validate-rates.typ target/template-rates.pdf

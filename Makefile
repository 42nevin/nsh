# Development tasks for working on nsh itself (see CONTRIBUTING.md).
#
#   make dev                          the gallery in the iOS simulator, hot reload
#   make dev TARGET=android           on an Android device or emulator
#   make dev TARGET=catalyst          in the Mac Catalyst window
#   make dev APP=examples/counter/App.tsx DEVICE="iPhone 17 Pro"
#
# JS and TSX changes reload in place (`nsh run`). Rust components and the
# rest of the runtime are compiled into the host, so `make dev` rebuilds it
# and restarts the app when anything under crates/ changes (needs
# cargo-watch: `cargo install cargo-watch`).

APP ?= examples/gallery/App.tsx
TARGET ?= ios
DEVICE ?=

NSH := ./target/debug/nsh
RUN_ARGS := $(APP) --target $(TARGET) $(if $(DEVICE),--device "$(DEVICE)")

HOST_catalyst := cargo build -p nsh-ios-host --target aarch64-apple-ios-macabi
HOST_ios := cargo build -p nsh-ios-host --target aarch64-apple-ios-sim
HOST_android := cargo xtask android --dev

.PHONY: dev run build test check

dev:
	@if command -v cargo-watch >/dev/null 2>&1; then \
		cargo watch -q -w crates -s '$(MAKE) --no-print-directory run'; \
	else \
		echo "cargo-watch not found (cargo install cargo-watch): Rust changes need a new make dev"; \
		$(MAKE) --no-print-directory run; \
	fi

run: build
	$(NSH) run $(RUN_ARGS)

build:
	@test -n "$(HOST_$(TARGET))" || { echo "TARGET must be ios, android or catalyst"; exit 1; }
	cargo build -p nsh-cli
	$(HOST_$(TARGET))

test:
	cargo test --workspace

check:
	cargo +nightly fmt --all --check
	cargo clippy --workspace --all-targets
	cargo clippy -p nsh-platform-ios -p nsh-ios-host -p nsh-ios-app --target aarch64-apple-ios-macabi --features nsh-platform-ios/embed,nsh-platform-ios/dev

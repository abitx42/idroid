SHELL := /bin/sh

CARGO ?= cargo
CARGO_LOCKED := --locked
CARGO_WORKSPACE := --workspace --all-targets

.PHONY: test lint check-clock-source clean test-mac test-offline

test:
	$(CARGO) test $(CARGO_LOCKED) --offline $(CARGO_WORKSPACE)

check-clock-source:
	@if grep -R -n -E 'std::time::Instant|Instant::now|use std::time' core/src; then \
		printf '%s\n' 'core timing must use the injected Clock trait' >&2; \
		exit 1; \
	fi

lint: check-clock-source
	$(CARGO) fmt --all -- --check
	$(CARGO) clippy $(CARGO_WORKSPACE) $(CARGO_LOCKED) -- -D warnings

clean:
	$(CARGO) clean

test-mac:
	@printf '%s\n' 'macOS check: running portable Rust core tests only.'
	@printf '%s\n' 'Native macOS application tests do not exist yet; a passing result does not verify either app scaffold.'
	$(CARGO) test $(CARGO_LOCKED) $(CARGO_WORKSPACE)
	@printf '%s\n' 'macOS check complete: portable core only; native app behavior remains unverified.'

test-offline:
	@printf '%s\n' 'NON-CLAIMING offline check: uses only the local Cargo cache and does not prove network independence or release readiness.'
	$(CARGO) test $(CARGO_LOCKED) --offline $(CARGO_WORKSPACE)

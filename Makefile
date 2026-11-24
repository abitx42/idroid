SHELL := /bin/sh

CARGO_LOCKED := --locked
CARGO_WORKSPACE := --workspace --all-targets
PINNED_CHANNEL := $(shell sed -n 's/^channel = "\(.*\)"/\1/p' rust-toolchain.toml)

# Prefer the rustup toolchain selected by rust-toolchain.toml over a
# Homebrew/system cargo on PATH. See research/audit.md A-014.
RUSTUP_ACTIVE := $(shell rustup show active-toolchain 2>/dev/null | awk '{print $$1}')
RUSTUP_BIN := $(HOME)/.rustup/toolchains/$(RUSTUP_ACTIVE)/bin
ifneq ($(wildcard $(RUSTUP_BIN)/cargo),)
CARGO ?= $(RUSTUP_BIN)/cargo
export RUSTC := $(RUSTUP_BIN)/rustc
export RUSTDOC := $(RUSTUP_BIN)/rustdoc
# `cargo clippy` resolves `clippy-driver` from PATH, so a Homebrew clippy on
# PATH would lint with a different compiler than the pinned one. See A-021.
export PATH := $(RUSTUP_BIN):$(PATH)
else
CARGO ?= cargo
endif

.PHONY: test lint check-clock-source check-headers check-toolchain clean test-mac test-offline research-check

check-headers:
	@if ! diff -q core-ffi/include/idroid_core_ffi.h apps/mac-native/Sources/IdroidCoreFFI/include/idroid_core_ffi.h >/dev/null; then \
		printf '%s\n' 'FFI headers differ: core-ffi/include/idroid_core_ffi.h and apps/mac-native/Sources/IdroidCoreFFI/include/idroid_core_ffi.h must stay identical.' >&2; \
		exit 1; \
	fi

check-toolchain:
	@actual=$$($(CARGO) --version | awk '{print $$2}'); \
	if [ "$$actual" != "$(PINNED_CHANNEL)" ]; then \
		printf '%s\n' "toolchain mismatch: rust-toolchain.toml pins $(PINNED_CHANNEL), but $(CARGO) reports $$actual." >&2; \
		printf '%s\n' "Install the pinned toolchain (rustup toolchain install $(PINNED_CHANNEL)) or run via a rustup-managed cargo." >&2; \
		exit 1; \
	fi

test: check-toolchain
	$(CARGO) test $(CARGO_LOCKED) --offline $(CARGO_WORKSPACE)

check-clock-source:
	@if grep -R -n -E 'std::time::Instant|Instant::now|use std::time' core/src; then \
		printf '%s\n' 'core timing must use the injected Clock trait' >&2; \
		exit 1; \
	fi

lint: check-toolchain check-clock-source check-headers
	@expected="$(RUSTUP_BIN)/clippy-driver"; actual=$$(command -v clippy-driver); \
	if [ "$$actual" != "$$expected" ]; then \
		printf '%s\n' "clippy-driver resolves to $$actual, not $$expected (A-021)." >&2; \
		exit 1; \
	fi
	$(CARGO) fmt --all -- --check
	$(CARGO) clippy $(CARGO_WORKSPACE) $(CARGO_LOCKED) -- -D warnings
	python3 scripts/research_check.py

clean:
	$(CARGO) clean

test-mac: check-toolchain
	@printf '%s\n' 'macOS check: running portable Rust core tests only.'
	@printf '%s\n' 'Native macOS application tests do not exist yet; a passing result does not verify either app scaffold.'
	$(CARGO) test $(CARGO_LOCKED) $(CARGO_WORKSPACE)
	@printf '%s\n' 'macOS check complete: portable core only; native app behavior remains unverified.'

test-offline: check-toolchain
	@printf '%s\n' 'NON-CLAIMING offline check: uses only the local Cargo cache and does not prove network independence or release readiness.'
	$(CARGO) test $(CARGO_LOCKED) --offline $(CARGO_WORKSPACE)

# Structural check for research/: unique ids, valid statuses, complete index.
# It cannot verify that a conclusion is true, only that the memory is coherent.
research-check:
	python3 scripts/research_check.py

# SPDX-License-Identifier: Apache-2.0
# Copyright 2026 The llvm2cprover Authors

.PHONY: all
# test-e2e-goto-convert subsumes test-e2e: it also verifies the default output as its oracle.
all: format test test-e2e-goto-convert clippy

.PHONY: patch-deps
patch-deps:
	@git -C third_party/kani diff --quiet -- cprover_bindings && git -C third_party/kani apply ../kani.patch || true

.PHONY: release
release: patch-deps
	cargo build --release

.PHONY: format
format:
	cargo fmt --check

.PHONY: test
test: patch-deps
	cargo test

.PHONY: test-e2e
test-e2e: patch-deps
	cargo build --bin llvm2cprover
	./tests/run.py

.PHONY: test-e2e-goto-convert
test-e2e-goto-convert: patch-deps
	cargo build --bin llvm2cprover
	./tests/run.py --goto-convert

# Needs ESBMC 8.x or later; not part of `all`.
.PHONY: test-e2e-esbmc
test-e2e-esbmc: patch-deps
	cargo build --bin llvm2cprover
	./tests/run.py --esbmc

.PHONY: clippy
clippy: patch-deps
	cargo clippy --all-targets -- -D warnings

.PHONY: clean
clean:
	cargo clean

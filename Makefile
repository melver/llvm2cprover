# SPDX-License-Identifier: Apache-2.0
# Copyright 2026 The llvm2cprover Authors

.PHONY: all
all: format test test-e2e clippy

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

.PHONY: clippy
clippy: patch-deps
	cargo clippy --all-targets -- -D warnings

.PHONY: clean
clean:
	cargo clean

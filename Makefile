# SPDX-License-Identifier: Apache-2.0
# Copyright (C) 2026, Marco Elver <me@marcoelver.com>

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
	python3 tests/run.py

.PHONY: clippy
clippy: patch-deps
	cargo clippy -- \
		-A clippy::too_many_arguments \
		-A clippy::comparison_chain \
		-A clippy::manual_range_contains \
		-A clippy::type_complexity \
		-A clippy::uninlined-format-args \
		-D warnings

.PHONY: clean
clean:
	cargo clean

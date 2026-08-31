// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The llvm2cprover Authors
// ---
// expected = "TRANSLATION FAILED"
// description = "rejects an assigns helper that mutates nonlocal state"
// translator_flags = ["--enforce-contract", "increment"]
// ---

int value;

int increment(void) { return ++value; }
int *assigns_increment(void) {
    value += 10;
    return &value;
}

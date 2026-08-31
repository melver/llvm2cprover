// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The llvm2cprover Authors
// ---
// expected = "VERIFICATION FAILED"
// description = "detects a write outside a nonempty assigns frame"
// translator_flags = ["--enforce-contract", "set_value"]
// ---

int value;
int other;

int set_value(void) {
    int old_other = other;
    value = 1;
    return old_other;
}
int *assigns_set_value(void) { return &other; }

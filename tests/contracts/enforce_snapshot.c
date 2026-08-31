// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The llvm2cprover Authors
// ---
// expected = "VERIFICATION SUCCESSFUL"
// description = "discharges a two-state postcondition using a pre-state snapshot"
// translator_flags = ["--enforce-contract", "increment"]
// ---

int value;

int increment(void) { return ++value; }

int snapshot_increment(void) { return value; }
int ensures_increment(int old_value, int result) {
    return value == old_value + 1 && result == value;
}
int *assigns_increment(void) { return &value; }

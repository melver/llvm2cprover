// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The llvm2cprover Authors
// ---
// expected = "VERIFICATION SUCCESSFUL"
// description = "accepts an implementation that stays inside a nonempty frame"
// translator_flags = ["--enforce-contract", "set_value"]
// ---

int value;

int set_value(void) {
    value = 1;
    return value;
}
int *assigns_set_value(void) { return &value; }

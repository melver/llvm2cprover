// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The llvm2cprover Authors
// ---
// expected = "TRANSLATION FAILED"
// description = "rejects CBMC's unsupported void-target nonempty frame combination"
// translator_flags = ["--enforce-contract", "set_value"]
// ---

int value;

void set_value(int ignored) {
    (void)ignored;
    value = 1;
}
int *assigns_set_value(int ignored) {
    (void)ignored;
    return &value;
}

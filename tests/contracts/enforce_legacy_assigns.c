// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The llvm2cprover Authors
// ---
// expected = "TRANSLATION FAILED"
// description = "rejects legacy imperative assigns helpers for discharge"
// translator_flags = ["--enforce-contract", "set_value"]
// ---

int value;

void set_value(void) { value = 1; }
void assigns_set_value(void) { value = 0; }


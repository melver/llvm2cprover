// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The llvm2cprover Authors
// ---
// expected = "TRANSLATION FAILED"
// description = "rejects an assigns helper without a concrete pointee type"
// translator_flags = ["--enforce-contract", "target"]
// ---

int target(void) { return 0; }
void *assigns_target(void) { return (void *)0; }

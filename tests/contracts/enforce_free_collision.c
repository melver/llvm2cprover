// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The llvm2cprover Authors
// ---
// expected = "TRANSLATION FAILED"
// description = "rejects collision with the private DFCC free placeholder"
// translator_flags = ["--enforce-contract", "target"]
// ---

int __llvm2cprover_unused_builtin_free;
int target(void) { return 0; }

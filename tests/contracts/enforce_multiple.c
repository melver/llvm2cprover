// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The llvm2cprover Authors
// ---
// expected = "TRANSLATION FAILED"
// description = "rejects more than one enforcement target per artifact"
// translator_flags = ["--enforce-contract", "first", "--enforce-contract", "second"]
// ---

int first(void) { return 1; }
int second(void) { return 2; }

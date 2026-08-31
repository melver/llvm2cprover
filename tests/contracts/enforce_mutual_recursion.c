// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The llvm2cprover Authors
// ---
// expected = "TRANSLATION FAILED"
// description = "rejects mutual recursion involving the enforced target"
// translator_flags = ["--enforce-contract", "first"]
// ---

int second(int);
int first(int value) { return value == 0 ? 0 : second(value - 1); }
int second(int value) { return value == 0 ? 0 : first(value - 1); }

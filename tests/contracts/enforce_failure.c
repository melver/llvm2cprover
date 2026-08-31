// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The llvm2cprover Authors
// ---
// expected = "VERIFICATION FAILED"
// description = "rejects an implementation that violates its postcondition"
// translator_flags = ["--enforce-contract", "increment"]
// ---

int increment(int value) { return value + 1; }

int ensures_increment(int value, int result) { return result == value + 2; }


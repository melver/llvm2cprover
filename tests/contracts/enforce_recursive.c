// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The llvm2cprover Authors
// ---
// expected = "TRANSLATION FAILED"
// description = "rejects recursive contract enforcement"
// translator_flags = ["--enforce-contract", "sum"]
// ---

int sum(int value) { return value == 0 ? 0 : value + sum(value - 1); }


// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The llvm2cprover Authors
// ---
// expected = "TRANSLATION FAILED"
// description = "rejects selecting replacement and enforcement together"
// translator_flags = ["--enforce-contract", "target", "--replace-with-contract", "target"]
// ---

int target(void) { return 1; }

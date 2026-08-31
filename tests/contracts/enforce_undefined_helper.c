// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The llvm2cprover Authors
// ---
// expected = "TRANSLATION FAILED"
// description = "rejects unresolved helpers in an enforcement obligation"
// translator_flags = ["--enforce-contract", "target"]
// ---

int target(void) { return 0; }
extern int ensures_target(int);

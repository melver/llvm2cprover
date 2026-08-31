// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The llvm2cprover Authors
// ---
// expected = "TRANSLATION FAILED"
// description = "rejects discharge of a trusted model"
// translator_flags = ["--enforce-contract", "target"]
// ---

int target(void) { return 1; }
int model_target(void) { return 2; }

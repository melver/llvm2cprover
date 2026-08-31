// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The llvm2cprover Authors
// ---
// expected = "TRANSLATION FAILED"
// description = "rejects enforcement of an external declaration"
// translator_flags = ["--enforce-contract", "target"]
// ---

extern int target(void);
int caller(void) { return target(); }

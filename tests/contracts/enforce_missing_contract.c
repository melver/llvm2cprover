// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The llvm2cprover Authors
// ---
// expected = "TRANSLATION FAILED"
// description = "rejects an enforcement selection without a manifest contract"
// translator_flags = ["--enforce-contract", "target"]
// ---

int target(void) { return 0; }

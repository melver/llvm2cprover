// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The llvm2cprover Authors
// ---
// expected = "TRANSLATION FAILED"
// description = "rejects input free uses that conflict with DFCC runtime linking"
// translator_flags = ["--enforce-contract", "target"]
// ---

extern void free(void *);
int target(void) { return 0; }
void release(void *pointer) { free(pointer); }

// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The llvm2cprover Authors
//
// ---
// description = "contract replacement makes requires a caller assertion"
// expected = "VERIFICATION FAILED"
// entry = "main"
// translator_flags = ["--replace-with-contract", "target"]
// ---

int target(int x) { return x; }
_Bool contract_requires_target(int x) { return x >= 0; }

int main(void) { return target(-1); }

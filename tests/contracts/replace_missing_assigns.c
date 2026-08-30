// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The llvm2cprover Authors
//
// ---
// description = "contract replacement rejects an omitted assigns frame"
// expected = "TRANSLATION FAILED"
// translator_flags = ["--replace-with-contract", "target"]
// ---

int target(void) { return 1; }
_Bool contract_ensures_target(int result) { return result == 1; }
int main(void) { return target(); }

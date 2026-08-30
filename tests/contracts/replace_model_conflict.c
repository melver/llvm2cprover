// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The llvm2cprover Authors
//
// ---
// description = "contract replacement rejects a simultaneous model"
// expected = "TRANSLATION FAILED"
// translator_flags = ["--replace-with-contract", "target"]
// ---

int target(void) { return 1; }
int contract_model_target(void) { return 2; }
int main(void) { return target(); }

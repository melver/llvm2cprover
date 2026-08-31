// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The llvm2cprover Authors
// ---
// expected = "TRANSLATION FAILED"
// description = "rejects enforcement of a variadic definition"
// translator_flags = ["--enforce-contract", "target"]
// ---

int target(int first, ...) { return first; }

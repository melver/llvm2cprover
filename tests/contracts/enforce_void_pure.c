// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The llvm2cprover Authors
// ---
// expected = "VERIFICATION SUCCESSFUL"
// description = "discharges an empty frame for a void implementation"
// translator_flags = ["--enforce-contract", "noop"]
// ---

void noop(void) {}

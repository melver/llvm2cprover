// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The llvm2cprover Authors
//
// ---
// description = "rename-prefix keeps pthread runtime declarations shared"
// expected = "VERIFICATION SUCCESSFUL"
// entry = "old_main"
// clang_flags = ["-pthread"]
// cbmc_flags = ["--no-standard-checks"]
// translator_flags = ["--rename-prefix", "old_"]
// ---

#include "../concurrency/pthread_create_join.c"

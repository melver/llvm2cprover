// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The llvm2cprover Authors
// ---
// description = "--fp-groups: an indirect call with no candidate in the cone translates; its result use still lowers; never reached here"
// expected = "VERIFICATION SUCCESSFUL"
// entry = "main"
// translator_flags = ["--entry", "main", "--fp-groups", "tests/functions/fp_groups_no_candidate.groups"]
// ---

int a_call(int (*hook)(int), int x) { return hook ? (hook(x) == 0) : -1; }

int main(void) { return a_call(0, 1) == -1 ? 0 : 1; }

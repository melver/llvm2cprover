// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The llvm2cprover Authors
// ---
// description = "--fp-groups: a dispatch candidate with a call-site contract fails closed"
// expected = "TRANSLATION FAILED"
// entry = "main"
// translator_flags = ["--entry", "main", "--fp-groups", "tests/functions/fp_groups.groups"]
// ---

int a_one(int x) { return x + 1; }
int b_one(int x) { return x * 100; }

_Bool contract_ensures_a_one(int x, int result) { return result == x + 1; }

int a_apply(int (*f)(int), int x) { return f(x); }

int main(void) { return a_apply(a_one, 1); }

// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The llvm2cprover Authors
// ---
// description = "--fp-groups: a group member that is not a function in the module fails closed (e.g. a typo or a missing --rename-prefix)"
// expected = "TRANSLATION FAILED"
// entry = "main"
// translator_flags = ["--entry", "main", "--fp-groups", "tests/functions/fp_groups_unknown.groups"]
// ---

int a_one(int x) { return x + 1; }
int b_one(int x) { return x * 100; }

int a_apply(int (*f)(int), int x) { return f(x); }

int main(void) {
  int (*volatile keep)(int) = a_one;
  (void)keep;
  return a_apply(b_one, 1);
}

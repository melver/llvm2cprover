// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The llvm2cprover Authors
// ---
// description = "--fp-groups: an indirect call in group a dispatches over group-a targets only"
// expected = "VERIFICATION SUCCESSFUL"
// entry = "main"
// translator_flags = ["--entry", "main", "--fp-groups", "tests/functions/fp_groups_dispatch.groups"]
// ---

extern int __CPROVER_nondet_int(void);
extern void __CPROVER_assume(_Bool);
extern void __CPROVER_assert(_Bool, const char*);

int a_one(int x) { return x + 1; }
int a_two(int x) { return x + 2; }
int b_one(int x) { return x * 100; }

int (*const table_a[2])(int) = {a_one, a_two};
int (*volatile other)(int) = b_one;  // same type, address taken, other group

int a_call(int i, int x) { return table_a[i](x); }

int main(void) {
  int i = __CPROVER_nondet_int();
  __CPROVER_assume(i == 0 || i == 1);
  __CPROVER_assert(a_call(i, 5) == (i ? 7 : 6), "group-a dispatch");
  return 0;
}

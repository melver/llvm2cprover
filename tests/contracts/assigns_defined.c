// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The llvm2cprover Authors
//
// ---
// description = "assigns metadata preserves an unselected defined function body"
// expected = "VERIFICATION SUCCESSFUL"
// entry = "main"
// ---

extern int __CPROVER_nondet_int(void);
extern void __CPROVER_assert(_Bool, const char*);

void target(int* value) { *value = 42; }

void contract_assigns_target(int* value) {
  *value = __CPROVER_nondet_int();
}

int main(void) {
  int value = 0;
  target(&value);
  __CPROVER_assert(value == 42, "the real target body executes");
  return 0;
}

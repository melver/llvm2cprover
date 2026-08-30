// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The llvm2cprover Authors
//
// ---
// description = "explicit replacement takes precedence over special-call lowering"
// expected = "VERIFICATION SUCCESSFUL"
// entry = "main"
// translator_flags = ["--replace-with-contract", "__CPROVER_nondet_int"]
// ---

extern int __CPROVER_nondet_int(void);
extern void __CPROVER_assert(_Bool, const char*);

_Bool contract_ensures_nondet_int(int result) { return result == 7; }

int main(void) {
  int result = __CPROVER_nondet_int();
  __CPROVER_assert(result == 7, "replacement ensures constrain the special call");
  return 0;
}

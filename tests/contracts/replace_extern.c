// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The llvm2cprover Authors
//
// ---
// description = "contract replacement synthesizes an external target wrapper"
// expected = "VERIFICATION SUCCESSFUL"
// entry = "main"
// translator_flags = ["--replace-with-contract", "target"]
// ---

extern int __CPROVER_nondet_int(void);
extern void __CPROVER_assert(_Bool, const char*);

extern int target(int* writable);

void contract_assigns_target(int* writable) {
  *writable = __CPROVER_nondet_int();
}

_Bool contract_ensures_target(int* writable, int result) {
  (void)writable;
  return result == 7;
}

int main(void) {
  int writable = 0;
  int result = target(&writable);
  __CPROVER_assert(result == 7, "the external replacement assumes ensures");
  return 0;
}

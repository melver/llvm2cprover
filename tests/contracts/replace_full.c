// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The llvm2cprover Authors
//
// ---
// description = "contract replacement checks requires, havocs assigns, and assumes ensures"
// expected = "VERIFICATION SUCCESSFUL"
// entry = "main"
// translator_flags = ["--replace-with-contract", "target"]
// ---

extern void __CPROVER_assert(_Bool, const char*);

int target_body_ran;

int target(int x, int* writable, int* untouched) {
  target_body_ran = 1;
  *untouched = 99;
  return x - 1;
}

_Bool contract_requires_target(int x, int* writable, int* untouched) {
  (void)writable;
  (void)untouched;
  return x >= 0;
}

int* contract_assigns_target(int x, int* writable, int* untouched) {
  (void)x;
  (void)untouched;
  return writable + 0;
}

_Bool contract_ensures_target(int x, int* writable, int* untouched, int result) {
  (void)writable;
  (void)untouched;
  return result == x + 1;
}

int main(void) {
  int writable = 0;
  int untouched = 7;
  int result = target(4, &writable, &untouched);
  __CPROVER_assert(result == 5, "ensures constrains the nondeterministic result");
  __CPROVER_assert(untouched == 7, "only assigns locations are changed");
  __CPROVER_assert(target_body_ran == 0, "the original body is bypassed");
  return 0;
}

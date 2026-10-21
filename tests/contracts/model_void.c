// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The llvm2cprover Authors
//
// ---
// description = "model helper replaces a void-returning direct call target"
// expected = "VERIFICATION SUCCESSFUL"
// entry = "main"
// ---

extern void __CPROVER_assert(_Bool, const char*);

void set_out(int* out) {
  *out = 0;
}

void contract_model_set_out(int* out) {
  *out = 42;
}

int main(void) {
  int value = 0;
  set_out(&value);
  __CPROVER_assert(value == 42, "void model helper updates caller state");
  return 0;
}

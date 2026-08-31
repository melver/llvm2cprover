// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The llvm2cprover Authors
//
// ---
// description = "snapshot values retain opaque pointers across a state change"
// expected = "VERIFICATION SUCCESSFUL"
// entry = "main"
// ---

extern void __CPROVER_assert(_Bool, const char*);

int before_value;
int after_value;

void redirect(int** slot) {
  *slot = &after_value;
}

void* snapshot_redirect(int** slot) {
  return *slot;
}

_Bool ensures_redirect(int** slot, int* before) {
  return before == &before_value && *slot == &after_value;
}

int main(void) {
  int* slot = &before_value;
  redirect(&slot);
  __CPROVER_assert(slot == &after_value, "real body redirects the pointer");
  return 0;
}

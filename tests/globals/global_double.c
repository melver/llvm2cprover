// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The llvm2cprover Authors
// ---
// description = "Non-zero double global initializers lower as constant expressions"
// expected = "VERIFICATION SUCCESSFUL"
// entry = "main"
// ---

extern void __CPROVER_assert(_Bool cond, const char* message);

static double g = 3.25;

int main(void) {
  __CPROVER_assert(g == 3.25, "global double initializer");
  return 0;
}

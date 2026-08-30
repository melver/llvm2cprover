// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The llvm2cprover Authors
// ---
// description = "Basic floating-point arithmetic and comparisons lower from C"
// expected = "VERIFICATION SUCCESSFUL"
// entry = "main"
// ---

extern void __CPROVER_assert(_Bool cond, const char* message);

int main(void) {
  float sum = 1.5f + 2.25f;
  double quo = 7.5 / 2.5;
  __CPROVER_assert(sum == 3.75f, "float add");
  __CPROVER_assert(quo == 3.0, "double div");
  __CPROVER_assert(sum > 3.0f, "float compare");
  return 0;
}

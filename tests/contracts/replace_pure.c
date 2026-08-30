// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The llvm2cprover Authors
//
// ---
// description = "an explicitly empty assigns frame models a pure replacement"
// expected = "VERIFICATION SUCCESSFUL"
// entry = "main"
// translator_flags = ["--replace-with-contract", "target"]
// ---

extern void __CPROVER_assert(_Bool, const char*);

int state;
void target(void) { state = 1; }

int main(void) {
  target();
  __CPROVER_assert(state == 0, "pure replacement preserves state");
  return 0;
}

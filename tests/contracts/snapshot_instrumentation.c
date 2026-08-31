// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The llvm2cprover Authors
//
// ---
// description = "snapshot preserves pointer-reachable state across a real call"
// expected = "VERIFICATION SUCCESSFUL"
// entry = "main"
// ---

extern void __CPROVER_assert(_Bool, const char*);

int increment(int* counter) {
  *counter += 1;
  return *counter;
}

int snapshot_increment(int* counter) {
  return *counter;
}

_Bool ensures_increment(int* counter, int before, int result) {
  return *counter == before + 1 && result == *counter;
}

int main(void) {
  int counter = 41;
  int result = increment(&counter);
  __CPROVER_assert(counter == 42, "real body updates the counter");
  __CPROVER_assert(result == 42, "real body returns the new counter");
  return 0;
}

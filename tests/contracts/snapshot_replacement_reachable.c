// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The llvm2cprover Authors
//
// ---
// description = "replacement snapshot is captured before havoc without pruning the caller"
// expected = "VERIFICATION FAILED"
// entry = "main"
// translator_flags = ["--replace-with-contract", "increment"]
// ---

extern int __CPROVER_nondet_int(void);
extern void __CPROVER_assert(_Bool, const char*);

int increment(int* counter) {
  *counter += 100;
  return *counter;
}

int snapshot_increment(int* counter) {
  return *counter;
}

void assigns_increment(int* counter) {
  *counter = __CPROVER_nondet_int();
}

_Bool ensures_increment(int* counter, int before, int result) {
  return *counter == before + 1 && result == *counter;
}

int main(void) {
  int counter = 41;
  (void)increment(&counter);
  __CPROVER_assert(0, "the post-contract caller remains reachable");
  return 0;
}

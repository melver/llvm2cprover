// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The llvm2cprover Authors
//
// ---
// description = "snapshot preserves pointer-reachable state across replacement havoc"
// expected = "VERIFICATION SUCCESSFUL"
// entry = "main"
// translator_flags = ["--replace-with-contract", "increment"]
// ---

extern void __CPROVER_assert(_Bool, const char*);

int body_ran;

int increment(int* counter) {
  body_ran = 1;
  *counter += 100;
  return *counter;
}

int snapshot_increment(int* counter) {
  return *counter;
}

int* assigns_increment(int* counter) { return counter + 0; }

_Bool ensures_increment(int* counter, int before, int result) {
  return *counter == before + 1 && result == *counter;
}

int main(void) {
  int counter = 41;
  int result = increment(&counter);
  __CPROVER_assert(counter == 42, "ensures relates post-state to captured pre-state");
  __CPROVER_assert(result == 42, "ensures constrains the replacement result");
  __CPROVER_assert(body_ran == 0, "the original body is bypassed");
  return 0;
}

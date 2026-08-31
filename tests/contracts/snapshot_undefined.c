// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The llvm2cprover Authors
//
// ---
// description = "snapshot helper must be a compiled definition"
// expected = "TRANSLATION FAILED"
// ---

extern int snapshot_target(int* value);

int target(int* value) {
  return *value;
}

_Bool ensures_target(int* value, int before, int result) {
  return *value == before && result == before;
}

int main(void) {
  int value = 1;
  return target(&value);
}

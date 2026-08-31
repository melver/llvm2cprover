// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The llvm2cprover Authors
//
// ---
// description = "snapshot helper must return a value"
// expected = "TRANSLATION FAILED"
// ---

int target(int* value) {
  return *value;
}

void snapshot_target(int* value) {
  (void)value;
}

_Bool ensures_target(int* value, int before, int result) {
  return *value == before && result == before;
}

int main(void) {
  int value = 1;
  return target(&value);
}

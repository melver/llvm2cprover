// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The llvm2cprover Authors
//
// ---
// description = "ensures snapshot parameter must match the captured value"
// expected = "TRANSLATION FAILED"
// ---

int target(int* value) {
  return *value;
}

long snapshot_target(int* value) {
  return *value;
}

_Bool ensures_target(int* value, int before, int result) {
  return *value == before && result == before;
}

int main(void) {
  int value = 1;
  return target(&value);
}

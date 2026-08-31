// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The llvm2cprover Authors
//
// ---
// description = "snapshot helper must not resolve only to a predeclared builtin"
// expected = "TRANSLATION FAILED"
// ---

double target(double value) {
  return value;
}

_Bool ensures_target(double value, double before, double result) {
  return value == before && result == value;
}

int main(void) {
  return target(1.0) != 1.0;
}

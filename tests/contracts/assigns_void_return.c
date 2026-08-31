// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The llvm2cprover Authors
// ---
// expected = "TRANSLATION FAILED"
// description = "assigns helpers cannot use an imperative void-returning form"
// ---

int value;

void set_value(void) { value = 1; }
void assigns_set_value(void) {}

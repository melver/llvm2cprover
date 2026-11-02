// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The llvm2cprover Authors
// ---
// description = "an i32 model stands in for a c_int-typed libc symbol (pthread_join); reaching its unreachable body fails"
// expected = "VERIFICATION FAILED"
// entry = "main"
// ---

#include <pthread.h>

int model_pthread_join(pthread_t thread, void** ret) { __builtin_unreachable(); }

int main(void) {
  pthread_t t = 0;
  return pthread_join(t, 0);
}

// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The llvm2cprover Authors
// ---
// description = "an i32 model's result is cast to a c_int-typed libc symbol's return (pthread_join) without changing its value"
// expected = "VERIFICATION SUCCESSFUL"
// entry = "main"
// ---

#include <pthread.h>

extern void __CPROVER_assert(_Bool, const char *);

int model_pthread_join(pthread_t thread, void** ret) { return -1; }

int main(void) {
  pthread_t t = 0;
  __CPROVER_assert(pthread_join(t, 0) == -1, "model result survives the signedness cast");
  return 0;
}

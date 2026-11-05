// ---
// description = "__CPROVER_atomic_begin/end sections do not interleave"
// expected = "VERIFICATION SUCCESSFUL"
// entry = "main"
// clang_flags = ["-pthread"]
// cbmc_flags = ["--no-standard-checks"]
// esbmc_skip = "ESBMC has no pthread models for CBMC binaries"
// ---
//
// Without atomicity, main could observe the write of x but not yet that of y.

#include <pthread.h>

extern void __CPROVER_assert(_Bool cond, const char* message);
extern void __CPROVER_atomic_begin(void);
extern void __CPROVER_atomic_end(void);

static int x, y;

static void* writer(void* arg) {
  (void)arg;
  __CPROVER_atomic_begin();
  x = 1;
  y = 1;
  __CPROVER_atomic_end();
  return 0;
}

int main(void) {
  pthread_t thread;
  pthread_create(&thread, 0, writer, 0);
  __CPROVER_atomic_begin();
  int seen_x = x, seen_y = y;
  __CPROVER_atomic_end();
  __CPROVER_assert(seen_x == seen_y, "atomic section observed whole");
  pthread_join(thread, 0);
  return 0;
}

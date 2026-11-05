// ---
// description = "SV-COMP __VERIFIER_atomic_* function bodies do not interleave"
// expected = "VERIFICATION SUCCESSFUL"
// entry = "main"
// clang_flags = ["-pthread"]
// cbmc_flags = ["--no-standard-checks"]
// esbmc_skip = "ESBMC has no pthread models for CBMC binaries"
// ---

#include <pthread.h>

extern void __CPROVER_assert(_Bool cond, const char* message);

static int x, y;

void __VERIFIER_atomic_write(void) {
  x = 1;
  y = 1;
}

int __VERIFIER_atomic_same(void) { return x == y; }

static void* writer(void* arg) {
  (void)arg;
  __VERIFIER_atomic_write();
  return 0;
}

int main(void) {
  pthread_t thread;
  pthread_create(&thread, 0, writer, 0);
  __CPROVER_assert(__VERIFIER_atomic_same(), "atomic function observed whole");
  pthread_join(thread, 0);
  return 0;
}

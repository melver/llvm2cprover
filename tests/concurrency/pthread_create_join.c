// ---
// description = "pthread_create and pthread_join expose plain shared-memory effects"
// expected = "VERIFICATION SUCCESSFUL"
// entry = "main"
// clang_flags = ["-pthread"]
// cbmc_flags = ["--no-standard-checks"]
// ---

#include <pthread.h>

extern void __CPROVER_assert(_Bool cond, const char* message);

static int x;

static void* writer(void* arg) {
  (void)arg;
  x = 1;
  return 0;
}

int main(void) {
  pthread_t thread;
  pthread_create(&thread, 0, writer, 0);
  pthread_join(thread, 0);
  __CPROVER_assert(x == 1, "joined thread published write");
  return 0;
}

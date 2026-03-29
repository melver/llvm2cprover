// ---
// description = "release/acquire publication preserves dependent shared-memory writes"
// expected = "VERIFICATION SUCCESSFUL"
// entry = "main"
// clang_flags = ["-pthread"]
// cbmc_flags = ["--no-standard-checks", "--unwind", "8"]
// ---

#include <pthread.h>
#include <stdatomic.h>

extern void __CPROVER_assert(_Bool cond, const char* message);

static atomic_int flag;
static int x;

static void* writer(void* arg) {
  (void)arg;
  x = 1;
  atomic_store_explicit(&flag, 1, memory_order_release);
  return 0;
}

int main(void) {
  pthread_t thread;
  pthread_create(&thread, 0, writer, 0);

  for (int i = 0; i < 8 && !atomic_load_explicit(&flag, memory_order_acquire); ++i) {}

  pthread_join(thread, 0);
  __CPROVER_assert(atomic_load_explicit(&flag, memory_order_relaxed) == 1, "flag set");
  __CPROVER_assert(x == 1, "acquire load observes published write");
  return 0;
}

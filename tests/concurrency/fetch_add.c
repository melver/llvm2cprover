// ---
// description = "atomic_fetch_add composes across two threads"
// expected = "VERIFICATION SUCCESSFUL"
// entry = "main"
// clang_flags = ["-pthread"]
// cbmc_flags = ["--no-standard-checks"]
// ---

#include <pthread.h>
#include <stdatomic.h>

extern void __CPROVER_assert(_Bool cond, const char* message);

static atomic_int counter;

static void* worker(void* arg) {
  (void)arg;
  atomic_fetch_add_explicit(&counter, 1, memory_order_seq_cst);
  return 0;
}

int main(void) {
  pthread_t a;
  pthread_t b;
  pthread_create(&a, 0, worker, 0);
  pthread_create(&b, 0, worker, 0);
  pthread_join(a, 0);
  pthread_join(b, 0);
  __CPROVER_assert(atomic_load_explicit(&counter, memory_order_seq_cst) == 2, "fetch_add");
  return 0;
}

// ---
// description = "release/acquire atomics lower correctly even without pthread scheduling"
// expected = "VERIFICATION SUCCESSFUL"
// entry = "main"
// ---

#include <stdatomic.h>

extern void __CPROVER_assert(_Bool cond, const char* message);

static atomic_int flag;
static int x;

int main(void) {
  x = 1;
  atomic_store_explicit(&flag, 1, memory_order_release);
  __CPROVER_assert(atomic_load_explicit(&flag, memory_order_acquire) == 1, "acquire load sees release store");
  __CPROVER_assert(x == 1, "ordinary state survives around atomics");
  return 0;
}

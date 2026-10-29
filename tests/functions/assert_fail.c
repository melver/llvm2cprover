// ---
// description = "a failing C assert() lowers __assert_fail to a property failure"
// expected = "VERIFICATION FAILED"
// entry = "main"
// ---
//
// Without -DNDEBUG, glibc's <assert.h> calls __assert_fail(assertion, file, line,
// function) on failure.

#include <assert.h>

extern int __CPROVER_nondet_int(void);

int main(void) {
  int x = __CPROVER_nondet_int();
  assert(x > 0);
  return 0;
}

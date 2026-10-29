// ---
// description = "a C assert() that holds does not reach __assert_fail"
// expected = "VERIFICATION SUCCESSFUL"
// entry = "main"
// ---

#include <assert.h>

extern int __CPROVER_nondet_int(void);
extern void __CPROVER_assume(_Bool);

int main(void) {
  int x = __CPROVER_nondet_int();
  __CPROVER_assume(x > 0);
  assert(x > 0);
  return 0;
}

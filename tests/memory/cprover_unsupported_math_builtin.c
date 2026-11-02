// ---
// description = "CBMC arithmetic expression builtins without a lowering fail closed too"
// expected = "TRANSLATION FAILED"
// entry = "main"
// ---

extern void __CPROVER_assert(_Bool, const char *);
extern _Bool __CPROVER_overflow_plus(int, int);
extern int __CPROVER_nondet_int(void);

int main(void) {
  int a = __CPROVER_nondet_int(), b = __CPROVER_nondet_int();
  __CPROVER_assert(!__CPROVER_overflow_plus(a, b) || a > 0 || b > 0 || a < 0, "overflow");
  return 0;
}

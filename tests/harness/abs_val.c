// ---
// description = "A small harness can verify an absolute-value implementation"
// expected = "VERIFICATION SUCCESSFUL"
// entry = "main"
// ---

extern int __CPROVER_nondet_int(void);
extern void __CPROVER_assume(_Bool cond);
extern void __CPROVER_assert(_Bool cond, const char* message);

static int abs_val(int x) {
  return x < 0 ? -x : x;
}

int main(void) {
  int x = __CPROVER_nondet_int();
  __CPROVER_assume(x > -100 && x < 100);
  __CPROVER_assert(abs_val(x) >= 0, "abs");
  return 0;
}

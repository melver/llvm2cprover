// ---
// description = "Nondeterministic harness values can be constrained with assume"
// expected = "VERIFICATION SUCCESSFUL"
// entry = "main"
// ---

extern void __CPROVER_assert(_Bool cond, const char* message);
extern void __CPROVER_assume(_Bool cond);
extern int __CPROVER_nondet_int(void);

int main(void) {
  int x = __CPROVER_nondet_int();
  __CPROVER_assume(x == 7);
  __CPROVER_assert(x == 7, "nondet");
  return 0;
}

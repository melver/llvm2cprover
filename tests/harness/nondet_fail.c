// ---
// description = "A nondeterministic value without enough assumptions can fail"
// expected = "VERIFICATION FAILED"
// entry = "main"
// ---

extern int __CPROVER_nondet_int(void);
extern void __CPROVER_assert(_Bool cond, const char* message);

int main(void) {
  int x = __CPROVER_nondet_int();
  __CPROVER_assert(x == 0, "nondet fail");
  return 0;
}

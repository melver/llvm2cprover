// ---
// description = "an assumed CBMC pointer builtin constrains later checks"
// expected = "VERIFICATION SUCCESSFUL"
// entry = "main"
// ---
//
// With the builtin as a bodiless nondet call, the assumption would constrain nothing.

extern void __CPROVER_assert(_Bool, const char *);
extern void __CPROVER_assume(_Bool);
extern _Bool __CPROVER_r_ok(const void *, unsigned long);
extern unsigned long __CPROVER_nondet_ulong(void);

char a[8];

int main(void) {
  unsigned long n = __CPROVER_nondet_ulong();
  __CPROVER_assume(__CPROVER_r_ok(a, n));
  __CPROVER_assert(n <= sizeof(a), "r_ok bounds n by the object size");
  return 0;
}

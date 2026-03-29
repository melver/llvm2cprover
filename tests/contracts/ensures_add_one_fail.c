// ---
// description = "ensures helper becomes a post-call assertion"
// expected = "VERIFICATION FAILED"
// entry = "main"
// ---

extern int __CPROVER_nondet_int(void);
extern void __CPROVER_assume(_Bool);

int add_one(int x) {
  return x + 2;
}

_Bool contract_ensures_add_one(int x, int result) {
  return result == x + 1;
}

int main(void) {
  int x = __CPROVER_nondet_int();
  __CPROVER_assume(-100 < x && x < 100);
  (void)add_one(x);
  return 0;
}

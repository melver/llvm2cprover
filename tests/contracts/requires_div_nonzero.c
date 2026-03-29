// ---
// description = "requires helper injects a call-site assume"
// expected = "VERIFICATION SUCCESSFUL"
// entry = "main"
// ---

extern int __CPROVER_nondet_int(void);

int reciprocal(int x) {
  return 10 / x;
}

_Bool contract_requires_reciprocal(int x) {
  return x != 0;
}

int main(void) {
  int x = __CPROVER_nondet_int();
  (void)reciprocal(x);
  return 0;
}

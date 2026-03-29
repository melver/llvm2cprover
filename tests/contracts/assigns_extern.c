// ---
// description = "assigns helpers both annotate and implement extern write effects"
// expected = "VERIFICATION SUCCESSFUL"
// entry = "main"
// ---

extern int __CPROVER_nondet_int(void);
extern void __CPROVER_assert(_Bool, const char*);

extern void scribble(int* x, int* y);

void contract_assigns_scribble(int* x, int* y) {
  *x = __CPROVER_nondet_int();
  (void)y;
}

int main(void) {
  int x = 1;
  int y = 2;
  scribble(&x, &y);
  __CPROVER_assert(y == 2, "assigns stub leaves y unchanged");
  return 0;
}

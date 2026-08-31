// ---
// description = "assigns helpers both annotate and implement extern write effects"
// expected = "VERIFICATION SUCCESSFUL"
// entry = "main"
// ---

extern void __CPROVER_assert(_Bool, const char*);

extern void scribble(int* x, int* y);

int* contract_assigns_scribble(int* x, int* y) {
  (void)y;
  return x + 0;
}

int main(void) {
  int x = 1;
  int y = 2;
  scribble(&x, &y);
  __CPROVER_assert(y == 2, "assigns stub leaves y unchanged");
  return 0;
}

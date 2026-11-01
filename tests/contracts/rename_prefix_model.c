// ---
// description = "--rename-prefix renames a model helper but keeps an external target name shared"
// expected = "VERIFICATION SUCCESSFUL"
// entry = "p_main"
// translator_flags = ["--rename-prefix", "p_"]
// ---

extern int __CPROVER_nondet_int(void);
extern void __CPROVER_assume(_Bool);
extern void __CPROVER_assert(_Bool, const char*);

extern int abs_val(int);

int contract_model_abs_val(int x) {
  return x < 0 ? -x : x;
}

int main(void) {
  int x = __CPROVER_nondet_int();
  __CPROVER_assume(-100 < x && x < 100);
  int y = abs_val(x);
  __CPROVER_assert(y >= 0, "abs model preserves non-negativity");
  return 0;
}

// ---
// description = "Stack allocation and load/store survive translation"
// expected = "VERIFICATION SUCCESSFUL"
// entry = "main"
// ---

extern void __CPROVER_assert(_Bool cond, const char* message);

int main(void) {
  int* p = __builtin_alloca(sizeof(*p));
  *p = 9;
  __CPROVER_assert(*p == 9, "store");
  return 0;
}

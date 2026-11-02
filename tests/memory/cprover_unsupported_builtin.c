// ---
// description = "CBMC builtins without a lowering fail closed instead of becoming bodiless calls"
// expected = "TRANSLATION FAILED"
// entry = "main"
// ---

extern void __CPROVER_assert(_Bool, const char *);
extern _Bool __CPROVER_is_invalid_pointer(const void *);

int x;

int main(void) {
  __CPROVER_assert(!__CPROVER_is_invalid_pointer(&x), "valid pointer");
  return 0;
}

// ---
// description = "Failing __CPROVER_assert is reported by CBMC"
// expected = "VERIFICATION FAILED"
// entry = "main"
// ---

extern void __CPROVER_assert(_Bool cond, const char* message);

int main(void) {
  __CPROVER_assert(0, "boom");
  return 0;
}

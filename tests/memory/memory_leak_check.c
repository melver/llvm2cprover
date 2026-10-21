// ---
// description = "CBMC --memory-leak-check does not crash when entry point makes no heap allocations"
// expected = "VERIFICATION SUCCESSFUL"
// entry = "main"
// cbmc_flags = ["--memory-leak-check"]
// ---

extern void __CPROVER_assert(_Bool cond, const char* message);

int main(void) {
  int x = 42;
  __CPROVER_assert(x == 42, "value matches");
  return 0;
}

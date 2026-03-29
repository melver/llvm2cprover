// ---
// description = "Recursive direct calls verify under an explicit unwind bound"
// expected = "VERIFICATION SUCCESSFUL"
// entry = "main"
// cbmc_flags = ["--unwind", "6"]
// ---

extern void __CPROVER_assert(_Bool cond, const char* message);

static int fact(int n) {
  if (n <= 1) {
    return 1;
  }
  return n * fact(n - 1);
}

int main(void) {
  __CPROVER_assert(fact(4) == 24, "recursive");
  return 0;
}

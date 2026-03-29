// ---
// description = "A small while-loop verifies with an explicit unwind bound"
// expected = "VERIFICATION SUCCESSFUL"
// entry = "main"
// cbmc_flags = ["--unwind", "5"]
// ---

extern void __CPROVER_assert(_Bool cond, const char* message);

int main(void) {
  int i = 0;
  int sum = 0;
  while (i < 4) {
    sum += i;
    ++i;
  }
  __CPROVER_assert(sum == 6, "while");
  return 0;
}

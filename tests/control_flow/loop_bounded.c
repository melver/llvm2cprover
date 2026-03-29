// ---
// description = "A small bounded loop verifies with an explicit unwind bound"
// expected = "VERIFICATION SUCCESSFUL"
// entry = "main"
// cbmc_flags = ["--unwind", "5"]
// ---

extern void __CPROVER_assert(_Bool cond, const char* message);

int main(void) {
  int sum = 0;
  for (int i = 0; i < 4; ++i) {
    sum += i;
  }
  __CPROVER_assert(sum == 6, "loop");
  return 0;
}

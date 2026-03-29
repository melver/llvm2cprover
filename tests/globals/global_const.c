// ---
// description = "Read-only global constants lower as immutable initializers"
// expected = "VERIFICATION SUCCESSFUL"
// entry = "main"
// ---

extern void __CPROVER_assert(_Bool cond, const char* message);

const int answer = 42;

int main(void) {
  __CPROVER_assert(answer == 42, "global const");
  return 0;
}

// ---
// description = "Two's complement negation lowers as integer subtraction from zero"
// expected = "VERIFICATION SUCCESSFUL"
// entry = "main"
// ---

extern void __CPROVER_assert(_Bool cond, const char* message);

int main(void) {
  int x = -7;
  __CPROVER_assert(-x == 7, "neg");
  return 0;
}

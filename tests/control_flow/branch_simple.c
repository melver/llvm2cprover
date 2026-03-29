// ---
// description = "Simple if/else path translated from C"
// expected = "VERIFICATION SUCCESSFUL"
// entry = "main"
// ---

extern void __CPROVER_assert(_Bool cond, const char* message);

int main(void) {
  int x = 1;
  if (x) {
    x = 2;
  } else {
    x = 3;
  }
  __CPROVER_assert(x == 2, "taken branch");
  return 0;
}

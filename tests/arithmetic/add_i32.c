// ---
// description = "Integer addition from C lowers and verifies"
// expected = "VERIFICATION SUCCESSFUL"
// entry = "main"
// ---

extern void __CPROVER_assert(_Bool cond, const char* message);

int main(void) {
  int x = 2;
  int y = 5;
  int z = x + y;
  __CPROVER_assert(z == 7, "sum");
  return 0;
}

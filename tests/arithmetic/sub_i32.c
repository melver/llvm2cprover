// ---
// description = "Integer subtraction lowers and verifies"
// expected = "VERIFICATION SUCCESSFUL"
// entry = "main"
// ---

extern void __CPROVER_assert(_Bool cond, const char* message);

int main(void) {
  __CPROVER_assert(9 - 4 == 5, "sub");
  return 0;
}

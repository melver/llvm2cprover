// ---
// description = "One-dimensional arrays lower and verify"
// expected = "VERIFICATION SUCCESSFUL"
// entry = "main"
// ---

extern void __CPROVER_assert(_Bool cond, const char* message);

int main(void) {
  int values[3] = {1, 2, 3};
  __CPROVER_assert(values[0] + values[2] == 4, "array1d");
  return 0;
}

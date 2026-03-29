// ---
// description = "Two-dimensional arrays lower as arrays of arrays"
// expected = "VERIFICATION SUCCESSFUL"
// entry = "main"
// ---

extern void __CPROVER_assert(_Bool cond, const char* message);

int main(void) {
  int values[2][2] = {
      {1, 2},
      {3, 4}
  };
  __CPROVER_assert(values[1][0] == 3, "array2d");
  return 0;
}

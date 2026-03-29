// ---
// description = "Arrays of structs preserve nested aggregate layout"
// expected = "VERIFICATION SUCCESSFUL"
// entry = "main"
// ---

extern void __CPROVER_assert(_Bool cond, const char* message);

struct pair {
  int x;
  int y;
};

int main(void) {
  struct pair values[2] = {
      {1, 2},
      {3, 4}
  };
  __CPROVER_assert(values[1].y == 4, "array of structs");
  return 0;
}

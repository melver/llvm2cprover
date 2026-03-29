// ---
// description = "Basic struct field access via GEP works"
// expected = "VERIFICATION SUCCESSFUL"
// entry = "main"
// ---

extern void __CPROVER_assert(_Bool cond, const char* message);

struct pair {
  int x;
  int y;
};

int main(void) {
  struct pair p = {2, 4};
  __CPROVER_assert(p.x + p.y == 6, "struct fields");
  return 0;
}

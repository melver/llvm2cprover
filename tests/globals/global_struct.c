// ---
// description = "Global struct initializers and field updates work"
// expected = "VERIFICATION SUCCESSFUL"
// entry = "main"
// ---

extern void __CPROVER_assert(_Bool cond, const char* message);

struct pair {
  int x;
  int y;
};

struct pair g = {1, 2};

int main(void) {
  g.y = g.x + 4;
  __CPROVER_assert(g.y == 5, "global struct");
  return 0;
}

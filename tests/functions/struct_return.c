// ---
// description = "Functions returning small structs lower through aggregate values"
// expected = "VERIFICATION SUCCESSFUL"
// entry = "main"
// ---

extern void __CPROVER_assert(_Bool cond, const char* message);

struct pair {
  int x;
  int y;
};

static struct pair make_pair(int x, int y) {
  struct pair out = {x, y};
  return out;
}

int main(void) {
  struct pair p = make_pair(2, 5);
  __CPROVER_assert(p.x == 2 && p.y == 5, "struct return");
  return 0;
}

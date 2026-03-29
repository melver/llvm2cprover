// ---
// description = "Functions taking structs by value preserve aggregate fields"
// expected = "VERIFICATION SUCCESSFUL"
// entry = "main"
// ---

extern void __CPROVER_assert(_Bool cond, const char* message);

struct pair {
  int x;
  int y;
};

static int sum_pair(struct pair p) {
  return p.x + p.y;
}

int main(void) {
  struct pair p = {7, 8};
  __CPROVER_assert(sum_pair(p) == 15, "struct arg");
  return 0;
}

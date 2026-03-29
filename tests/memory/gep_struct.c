// ---
// description = "Struct field access lowers through GEP and member selection"
// expected = "VERIFICATION SUCCESSFUL"
// entry = "main"
// ---

extern void __CPROVER_assert(_Bool cond, const char* message);

struct pair {
  int x;
  int y;
};

int main(void) {
  struct pair p = {3, 4};
  p.y = p.x + 2;
  __CPROVER_assert(p.y == 5, "struct gep");
  return 0;
}

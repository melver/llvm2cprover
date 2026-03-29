// ---
// description = "Signed and unsigned division preserve their distinct semantics"
// expected = "VERIFICATION SUCCESSFUL"
// entry = "main"
// ---

extern void __CPROVER_assert(_Bool cond, const char* message);

int main(void) {
  int s = -9 / 2;
  unsigned u = 9u / 2u;
  __CPROVER_assert(s == -4 && u == 4u, "div");
  return 0;
}

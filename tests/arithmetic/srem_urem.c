// ---
// description = "Signed and unsigned remainder preserve their distinct semantics"
// expected = "VERIFICATION SUCCESSFUL"
// entry = "main"
// ---

extern void __CPROVER_assert(_Bool cond, const char* message);

int main(void) {
  int s = -9 % 4;
  unsigned u = 9u % 4u;
  __CPROVER_assert(s == -1 && u == 1u, "rem");
  return 0;
}

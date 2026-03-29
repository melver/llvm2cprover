// ---
// description = "Shift operations lower with their LLVM integer semantics"
// expected = "VERIFICATION SUCCESSFUL"
// entry = "main"
// ---

extern void __CPROVER_assert(_Bool cond, const char* message);

int main(void) {
  unsigned x = 3u;
  int y = -8;
  __CPROVER_assert((x << 2) == 12u, "shl");
  __CPROVER_assert((x << 2 >> 1) == 6u, "lshr");
  __CPROVER_assert((y >> 1) == -4, "ashr");
  return 0;
}

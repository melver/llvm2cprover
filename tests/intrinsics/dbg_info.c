// ---
// description = "Debug metadata is accepted and preserved through translation"
// expected = "VERIFICATION SUCCESSFUL"
// entry = "main"
// clang_flags = ["-g"]
// ---

extern void __CPROVER_assert(_Bool cond, const char* message);

int main(void) {
  int value = 8;
  __CPROVER_assert(value == 8, "dbg");
  return 0;
}

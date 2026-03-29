// ---
// description = "__builtin_expect lowers through llvm.expect and preserves the value"
// expected = "VERIFICATION SUCCESSFUL"
// entry = "main"
// ---

extern void __CPROVER_assert(_Bool cond, const char* message);

int main(void) {
  int x = __builtin_expect(7, 0);
  __CPROVER_assert(x == 7, "expect");
  return 0;
}

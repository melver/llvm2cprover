// ---
// description = "C booleans lower through LLVM i1 values"
// expected = "VERIFICATION SUCCESSFUL"
// entry = "main"
// ---

extern void __CPROVER_assert(_Bool cond, const char* message);

int main(void) {
  _Bool flag = 1;
  __CPROVER_assert(flag, "bool");
  return 0;
}

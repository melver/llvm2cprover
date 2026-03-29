// ---
// description = "llvm.memmove preserves overlapping copies"
// expected = "VERIFICATION SUCCESSFUL"
// entry = "main"
// ---

extern void __CPROVER_assert(_Bool cond, const char* message);

int main(void) {
  char buf[4] = {'a', 'b', 'c', 0};
  __builtin_memmove(buf + 1, buf, 2);
  __CPROVER_assert(buf[0] == 'a', "buf0");
  __CPROVER_assert(buf[1] == 'a', "buf1");
  __CPROVER_assert(buf[2] == 'b', "buf2");
  return 0;
}

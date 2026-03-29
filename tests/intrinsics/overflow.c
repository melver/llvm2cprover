// ---
// description = "llvm.sadd.with.overflow from __builtin_add_overflow is modeled"
// expected = "VERIFICATION SUCCESSFUL"
// entry = "main"
// ---

extern void __CPROVER_assert(_Bool cond, const char* message);

int main(void) {
  int out = 0;
  _Bool overflow = __builtin_add_overflow(2147483647, 1, &out);
  __CPROVER_assert(overflow, "overflow");
  return 0;
}

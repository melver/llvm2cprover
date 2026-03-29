// ---
// description = "__builtin_memcpy lowers through llvm.memcpy"
// expected = "VERIFICATION SUCCESSFUL"
// entry = "main"
// ---

extern void* __builtin_memcpy(void* dest, const void* src, __SIZE_TYPE__ n);
extern void __CPROVER_assert(_Bool cond, const char* message);

int main(void) {
  int src[2] = {3, 5};
  int dst[2] = {0, 0};
  __builtin_memcpy(dst, src, sizeof(src));
  __CPROVER_assert(dst[0] == 3 && dst[1] == 5, "memcpy");
  return 0;
}

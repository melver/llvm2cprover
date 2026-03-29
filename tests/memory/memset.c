// ---
// description = "__builtin_memset lowers through llvm.memset"
// expected = "VERIFICATION SUCCESSFUL"
// entry = "main"
// ---

extern void* __builtin_memset(void* dest, int ch, __SIZE_TYPE__ count);
extern void __CPROVER_assert(_Bool cond, const char* message);

int main(void) {
  unsigned char bytes[3] = {0, 0, 0};
  __builtin_memset(bytes, 0x2A, sizeof(bytes));
  __CPROVER_assert(bytes[0] == 0x2A && bytes[2] == 0x2A, "memset");
  return 0;
}

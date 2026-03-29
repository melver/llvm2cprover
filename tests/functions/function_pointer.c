// ---
// description = "Indirect calls via function pointers lower through code pointers"
// expected = "VERIFICATION SUCCESSFUL"
// entry = "main"
// ---

extern void __CPROVER_assert(_Bool cond, const char* message);

static int add_one(int x) {
  return x + 1;
}

int main(void) {
  int (*fp)(int) = add_one;
  __CPROVER_assert(fp(41) == 42, "function pointer");
  return 0;
}

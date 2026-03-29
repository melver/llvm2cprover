// ---
// description = "Multi-argument direct calls preserve argument order and values"
// expected = "VERIFICATION SUCCESSFUL"
// entry = "main"
// ---

extern void __CPROVER_assert(_Bool cond, const char* message);

static int combine(int a, int b, int c) {
  return a + 10 * b + 100 * c;
}

int main(void) {
  __CPROVER_assert(combine(3, 2, 1) == 123, "multi arg");
  return 0;
}

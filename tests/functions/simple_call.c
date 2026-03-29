// ---
// description = "Direct function calls and return values lower from C"
// expected = "VERIFICATION SUCCESSFUL"
// entry = "main"
// ---

extern void __CPROVER_assert(_Bool cond, const char* message);

static int inc(int x) {
  return x + 1;
}

int main(void) {
  __CPROVER_assert(inc(4) == 5, "call");
  return 0;
}

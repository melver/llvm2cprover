// ---
// description = "__builtin_unreachable lowers to an explicit verification failure path"
// expected = "VERIFICATION SUCCESSFUL"
// entry = "main"
// ---

extern void __CPROVER_assert(_Bool cond, const char* message);

static int f(int x) {
  if (x == 0) {
    return 1;
  }
  __builtin_unreachable();
}

int main(void) {
  __CPROVER_assert(f(0) == 1, "unreachable");
  return 0;
}

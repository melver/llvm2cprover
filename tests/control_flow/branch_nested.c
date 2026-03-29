// ---
// description = "Nested conditionals lower through chained branches"
// expected = "VERIFICATION SUCCESSFUL"
// entry = "main"
// ---

extern void __CPROVER_assert(_Bool cond, const char* message);

static int f(int x) {
  if (x > 10) {
    if (x > 20) {
      return 3;
    }
    return 2;
  }
  return 1;
}

int main(void) {
  __CPROVER_assert(f(15) == 2, "nested branch");
  return 0;
}

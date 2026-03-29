// ---
// description = "Simple switch dispatch lowers through LLVM switch"
// expected = "VERIFICATION SUCCESSFUL"
// entry = "main"
// ---

extern void __CPROVER_assert(_Bool cond, const char* message);

static int classify(int x) {
  switch (x) {
  case 0:
    return 10;
  case 1:
    return 20;
  case 2:
    return 30;
  default:
    return 40;
  }
}

int main(void) {
  __CPROVER_assert(classify(2) == 30, "switch");
  return 0;
}

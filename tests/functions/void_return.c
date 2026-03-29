// ---
// description = "Void functions with side effects lower correctly"
// expected = "VERIFICATION SUCCESSFUL"
// entry = "main"
// ---

extern void __CPROVER_assert(_Bool cond, const char* message);

static void inc(int* value) {
  *value = *value + 1;
}

int main(void) {
  int x = 4;
  inc(&x);
  __CPROVER_assert(x == 5, "void return");
  return 0;
}

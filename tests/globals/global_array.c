// ---
// description = "Global array initializers and indexed updates work"
// expected = "VERIFICATION SUCCESSFUL"
// entry = "main"
// ---

extern void __CPROVER_assert(_Bool cond, const char* message);

int data[3] = {2, 4, 6};

int main(void) {
  data[2] = data[0] + data[1];
  __CPROVER_assert(data[2] == 6, "global array");
  return 0;
}

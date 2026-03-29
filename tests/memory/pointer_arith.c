// ---
// description = "Pointer arithmetic lowers through index-like address computation"
// expected = "VERIFICATION SUCCESSFUL"
// entry = "main"
// ---

extern void __CPROVER_assert(_Bool cond, const char* message);

int main(void) {
  int data[3] = {4, 5, 6};
  int* ptr = data;
  ptr = ptr + 2;
  __CPROVER_assert(*ptr == 6, "pointer arithmetic");
  return 0;
}

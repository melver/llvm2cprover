// ---
// description = "Double indirection lowers through nested loads"
// expected = "VERIFICATION SUCCESSFUL"
// entry = "main"
// ---

extern void __CPROVER_assert(_Bool cond, const char* message);

int main(void) {
  int value = 8;
  int* ptr = &value;
  int** pptr = &ptr;
  __CPROVER_assert(**pptr == 8, "double pointer");
  return 0;
}

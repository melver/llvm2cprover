// ---
// description = "String literals lower as global byte arrays with pointer access"
// expected = "VERIFICATION SUCCESSFUL"
// entry = "main"
// ---

extern void __CPROVER_assert(_Bool cond, const char* message);

int main(void) {
  const char* message = "abc";
  __CPROVER_assert(message[1] == 'b', "string literal");
  return 0;
}

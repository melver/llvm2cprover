// ---
// description = "External global declarations can be read and written"
// expected = "VERIFICATION SUCCESSFUL"
// entry = "main"
// ---

extern void __CPROVER_assert(_Bool cond, const char* message);

extern int ext;
int ext = 9;

int main(void) {
  ext = ext + 1;
  __CPROVER_assert(ext == 10, "extern global");
  return 0;
}

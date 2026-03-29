// ---
// description = "assigns helpers must return void"
// expected = "TRANSLATION FAILED"
// entry = "main"
// ---

extern void target(int* x);

int contract_assigns_target(int* x) {
  *x = 0;
  return 0;
}

int main(void) {
  return 0;
}

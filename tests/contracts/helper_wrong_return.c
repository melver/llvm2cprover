// ---
// description = "value helpers must return a value"
// expected = "TRANSLATION FAILED"
// entry = "main"
// ---

int target(int x) {
  return x + 1;
}

void contract_requires_target(int x) {
  (void)x;
}

int main(void) {
  return 0;
}

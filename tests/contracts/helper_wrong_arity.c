// ---
// description = "helper arity mismatches fail translation"
// expected = "TRANSLATION FAILED"
// entry = "main"
// ---

int target(int x) {
  return x + 1;
}

_Bool contract_requires_target(void) {
  return 1;
}

int main(void) {
  return 0;
}

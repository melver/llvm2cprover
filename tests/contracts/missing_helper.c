// ---
// description = "missing helpers fail translation even when the target is never called"
// expected = "TRANSLATION FAILED"
// entry = "main"
// ---

int never_called(int x) {
  return x + 1;
}

int main(void) {
  return 0;
}

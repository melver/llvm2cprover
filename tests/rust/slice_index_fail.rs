// ---
// description = "Invalid Rust subslice range lowers core::slice::index::slice_index_fail to a failure"
// expected = "VERIFICATION FAILED"
// entry = "harness"
// ---

fn subslice(values: &[i32], start: usize, end: usize) -> usize {
    values[start..end].len()
}

#[no_mangle]
pub extern "C" fn harness() -> usize {
    let arr = [10, 20];
    subslice(&arr, 1, 3)
}

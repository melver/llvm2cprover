// ---
// description = "Out-of-bounds Rust indexing lowers core::panicking::panic_bounds_check to a failure"
// expected = "VERIFICATION FAILED"
// entry = "harness"
// ---

fn index_array(values: &[i32; 2], idx: usize) -> i32 {
    values[idx]
}

#[no_mangle]
pub extern "C" fn harness() -> i32 {
    let arr = [10, 20];
    index_array(&arr, 2)
}

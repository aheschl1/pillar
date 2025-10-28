// Contract increments a counter stored in host storage each time it runs.

unsafe extern "C" {
    unsafe fn height_get() -> i32;
    unsafe fn height_set(height: u32) -> i32;
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn call() {
    let height = height_get();
    let new_height = height + 1;
    height_set(new_height as u32);
}

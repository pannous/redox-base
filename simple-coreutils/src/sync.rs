extern "C" {
    fn sync();
}

fn main() {
    unsafe {
        sync();
    }
}

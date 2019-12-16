use std::ptr::NonNull;
use std::sync::atomic::AtomicUsize;

struct Buf<T> {
    data: *mut T,
    read: AtomicUsize,
    write: AtomicUsize,
}

pub struct Producer<T> {
    buf: NonNull<Buf<T>>,
}

impl<T> Producer<T> {
    pub fn push(&mut self, T) {

    }
}

pub struct Consumer<T> {
    buf: NonNull<Buf<T>>,
}

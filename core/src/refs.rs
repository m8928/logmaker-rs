use std::ops::Deref;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

/// A registry entry that counts the logs and scenarios using it. Entries in
/// use cannot be deleted.
pub trait RefCounted {
    fn ref_counter(&self) -> &AtomicUsize;

    fn refs(&self) -> usize {
        self.ref_counter().load(Ordering::SeqCst)
    }
}

/// Usage handle: increments the entry's reference count while alive.
pub struct Ref<T: RefCounted>(Arc<T>);

impl<T: RefCounted> Ref<T> {
    /// Must be called while holding the registry lock, so a concurrent delete
    /// cannot remove the entry between its lookup and this increment.
    pub(crate) fn acquire(entry: &Arc<T>) -> Self {
        entry.ref_counter().fetch_add(1, Ordering::SeqCst);
        Self(Arc::clone(entry))
    }
}

impl<T: RefCounted> Clone for Ref<T> {
    fn clone(&self) -> Self {
        Self::acquire(&self.0)
    }
}

impl<T: RefCounted> Drop for Ref<T> {
    fn drop(&mut self) {
        self.0.ref_counter().fetch_sub(1, Ordering::SeqCst);
    }
}

impl<T: RefCounted> Deref for Ref<T> {
    type Target = T;

    fn deref(&self) -> &T {
        &self.0
    }
}

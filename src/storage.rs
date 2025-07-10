use core::ptr::NonNull;

use crate::Channel;

#[cfg(not(oneshot_loom))]
use crate::alloc::boxed::Box;
#[cfg(oneshot_loom)]
use crate::loombox::Box;

/// The mechanism used to manage the storage of the inner state of a channel of `T`.
#[expect(private_bounds, reason = "sealed trait with private API surface")]
pub trait Storage<T>: StoragePrivate<T> {}

/// Usage model is to treat the implementing type as if it were a `NonNull<Channel<T>>`.
///
/// That is, it can be cloned freely and every clone (pointer) points to the same underlying data.
/// Dropping the object itself only drops the object (pointer), not the thing it points to.
/// To drop the data within and release the storage capacity, `release()` must be called explicitly.
///
/// # Safety
///
/// Implementations must implement the usage model described above, acting as pointers.
pub(crate) unsafe trait StoragePrivate<T> {
    /// Initializes the storage with a new `Channel<T>`, overwriting existing contents.
    ///
    /// # Safety
    ///
    /// This must not be called more than once and must not be called after `release()`.
    #[expect(dead_code, reason = "future implementations will use this")]
    unsafe fn initialize(&mut self);

    /// Releases the capacity that provides this storage.
    ///
    /// This will drop the `Channel<T>` and invalidate all clones of this storage.
    ///
    /// This must be called exactly once for each family of clones to avoid resource leaks.
    ///
    /// # Safety
    ///
    /// This must not be called multiple times on the same family of clones.
    unsafe fn release(&mut self);

    /// Dereferences the stored `Channel<T>`.
    ///
    /// # Safety
    ///
    /// The caller must guarantee that `initialize()` has been called and `release()` has not been
    /// called on any of the clones of this storage.
    unsafe fn as_ref(&self) -> &Channel<T>;

    /// Clones the storage, returning a new instance that points to the same underlying data.
    ///
    /// This is implemented as an inherent method to avoid exposing the `Clone` trait
    /// to users of implementation types outside this crate. Only the logic in this crate
    /// should be cloning the storage objects.
    fn clone(&self) -> Self;
}

impl<S: StoragePrivate<T>, T> Storage<T> for S {}

/// The storage of the inner state of a channel is allocated via the Rust global allocator.
#[derive(Debug)]
pub struct Global<T> {
    ptr: NonNull<Channel<T>>,
}

impl<T> Global<T> {
    /// # Safety
    ///
    /// The caller must not call `initialize()` - on this implementation, `new()` implicitly
    /// calls `initialize()` already, so calling it again would violate the trait safety contract.
    pub(crate) unsafe fn new() -> Self {
        let ptr = NonNull::from(Box::leak(Box::new(Channel::new())));

        Global { ptr }
    }

    /// Obtains the raw heap pointer that this storage object wraps.
    ///
    /// Using this pointer, an equivalent storage object be reconstructed with `Global::from_raw()`.
    pub(crate) fn to_raw(&self) -> NonNull<Channel<T>> {
        self.ptr
    }

    /// Reconstructs a storage object previously created with `to_raw()`.
    ///
    /// # Safety
    ///
    /// All the type invariants must remain in place - the recreated storage object
    /// rejoins the same family of clones that it was created from.
    pub(crate) unsafe fn from_raw(raw: NonNull<Channel<T>>) -> Self {
        Global { ptr: raw }
    }
}

// SAFETY: We implement the "this is just a fancy pointer" model as required by the trait.
unsafe impl<T> StoragePrivate<T> for Global<T> {
    unsafe fn as_ref(&self) -> &Channel<T> {
        // SAFETY: Yes, our pointer is valid and points to a `Channel<T>`.
        // The caller is responsible for ensuring that `initialize()` has been called.
        unsafe { self.ptr.as_ref() }
    }

    unsafe fn initialize(&mut self) {
        // SAFETY: This is a valid location for a `Channel<T>`, and we are initializing it.
        // The caller is responsible for ensuring that this is not called more than once per family.
        // The caller is also responsible for ensuring that `release()` has not been called on the family.
        unsafe {
            self.ptr.as_ptr().write(Channel::new());
        }
    }

    unsafe fn release(&mut self) {
        dealloc(self.ptr);

        // We rely on safety requirements to ensure this is never used again.
        self.ptr = NonNull::dangling();
    }

    fn clone(&self) -> Self {
        Global { ptr: self.ptr }
    }
}

#[inline]
unsafe fn dealloc<T>(channel: NonNull<Channel<T>>) {
    drop(Box::from_raw(channel.as_ptr()))
}

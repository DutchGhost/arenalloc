use core::{
    alloc::{Layout, LayoutErr},
    cell::Cell,
    mem::{self, MaybeUninit},
    ptr::{self, NonNull},
};

use alloc::alloc::{alloc_zeroed, dealloc};

/// A Bucket is a bucket of bytes.
/// These bytes may be the backing
/// store of any type.
#[repr(C)]
struct BucketImpl {
    /// An index into the data field.
    /// This index wil *always* index
    /// the next free byte
    index: Cell<usize>,

    /// The data of a Node. This slice
    /// may be of arbitrary size. A
    /// MaybeUninit is used to be able
    /// to write padding bytes.
    data: [Cell<MaybeUninit<u8>>],
}

impl BucketImpl {
    fn header_layout() -> Layout {
        Layout::new::<Cell<usize>>()
    }

    /// Returns the layout for an array with the size of `size`
    fn data_layout(size: usize) -> Result<Layout, LayoutErr> {
        Layout::new::<Cell<MaybeUninit<u8>>>()
            .repeat(size)
            .map(|layout| layout.0)
    }

    /// Returns a layout for a Node where the length of the data field is `size`.
    /// This relies on the two functions defined above.
    fn layout_from_size(size: usize) -> Result<Layout, LayoutErr> {
        let layout = Self::header_layout().extend(Self::data_layout(size)?)?.0;
        Ok(layout.pad_to_align())
    }
}

impl BucketImpl {
    fn capacity(&self) -> usize {
        self.data.len()
    }

    fn is_full(&self) -> bool {
        self.index.get() == self.capacity()
    }
}

/// Represents an insufficient capacity
/// within a Bucket
#[derive(Debug)]
pub struct CapacityError;

impl BucketImpl {
    /// Returns the start *address* of the data field
    fn data_start_address(&self) -> usize {
        self.data.as_ptr() as usize + self.index.get()
    }

    /// Returns the *next* index that has the correct
    /// alignment in memory for T,
    fn align_index_for<T>(&self) -> usize {
        fn next_power_of(n: usize, pow: usize) -> usize {
            let remain = n % pow;

            [n, n + (pow - remain)][(remain != 0) as usize]
        }

        let start_addr = self.data_start_address();
        let aligned_start = next_power_of(start_addr, mem::align_of::<T>());
        let aligned_index = aligned_start - self.data.as_ptr() as usize;
        aligned_index
    }

    /// Allocates the space for any `T` at the correct
    /// alignment.
    /// ```skip
    /// [.., .., 0, 0, 0, 0, 0]
    ///          ^
    ///         index
    ///
    /// malloc::<u8>(3) results in:
    /// [.., .., 0, 0, 0, 0, 0]
    ///                   ^
    ///                 index
    /// ```
    fn malloc<T>(&self, size: usize) -> Result<*mut T, CapacityError> {
        let start = self.align_index_for::<T>();

        // TODO: This could overflow?
        let total_alloc_size = mem::size_of::<T>() * size;

        let ptr = match self
            .data
            .get(start..)
            .and_then(|slice| slice.get(..mem::size_of::<T>() * size))
            .map(|place| {
                let ptr = place.as_ptr() as *mut T;
                assert!(ptr as usize % mem::align_of::<T>() == 0);
                ptr
            }) {
            Some(ptr) => ptr,
            None => return Err(CapacityError),
        };

        let end = start.saturating_add(total_alloc_size);
        self.index.set(end);
        Ok(ptr)
    }
}

#[derive(Debug)]
pub struct RawAllocError;

impl BucketImpl {
    unsafe fn alloc_raw(layout: Layout) -> Result<*mut u8, RawAllocError> {
        let ptr = alloc_zeroed(layout);

        if ptr.is_null() {
            Err(RawAllocError)
        } else {
            Ok(ptr)
        }
    }

    unsafe fn dealloc_raw(this: NonNull<Self>) {
        let size = this.as_ref().capacity();

        let layout =
            Self::layout_from_size(size).expect("Failed to construct layout for allocated Bump");

        dealloc(this.as_ptr() as *mut u8, layout);
    }
}

pub(crate) struct Bucket {
    // TODO: Make this an Option,
    // and `.take()` it in in the
    // Drop impl? This gives more
    // safety??
    ptr: NonNull<BucketImpl>,
}

impl Bucket {
    /// Allocates a Bucket and returns it.
    pub(super) fn new(size: usize) -> Result<Self, RawAllocError> {
        let layout = BucketImpl::layout_from_size(size).map_err(|_| RawAllocError)?;

        unsafe {
            let ptr = BucketImpl::alloc_raw(layout)?;

            let raw_mut: *mut [Cell<MaybeUninit<u8>>] =
                ptr::slice_from_raw_parts_mut(ptr.cast(), size);

            let node_ptr = raw_mut as *mut BucketImpl;

            Ok(Self {
                ptr: NonNull::new_unchecked(node_ptr),
            })
        }
    }

    pub(super) fn capacity(&self) -> usize {
        unsafe { self.ptr.as_ref().capacity() }
    }

    pub(super) fn is_full(&self) -> bool {
        unsafe { self.ptr.as_ref().is_full() }
    }

    pub(super) fn malloc<T>(&self, size: usize) -> Result<*mut T, CapacityError> {
        unsafe { self.ptr.as_ref().malloc(size) }
    }
}

impl Drop for Bucket {
    fn drop(&mut self) {
        unsafe {
            BucketImpl::dealloc_raw(self.ptr);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Bucket;

    #[test]
    fn test_malloc() {
        let b = Bucket::new(12).unwrap();
        let _ptr = b.malloc::<u8>(1).unwrap();

        let _otherptr = b.malloc::<u32>(2).unwrap();

        assert!(b.is_full());
        assert!(b.malloc::<u8>(1).is_err());
    }
}

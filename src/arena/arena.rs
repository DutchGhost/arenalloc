use core::{
    cell::{Cell, Ref, RefCell},
    marker::PhantomData,
};

use alloc::{vec, vec::Vec};

use super::bucket::{Bucket, CapacityError};
/// An Arena is just a Vector of buckets:
/// ```skip
/// [b1,    b2,     b3,     b4,     b5]
///  |      |       |       |       |
///  |      |       |       |       |
/// [....] [...]  [....]  [....]  [....]
/// ```
pub struct Arena {
    /// An index into the current used
    /// bucket. This index is always
    /// a valid index into te buckets
    index: Cell<usize>,

    /// The buckets in the Arena
    buckets: RefCell<Vec<Bucket>>,
}

#[derive(Copy, Clone)]
pub struct Scope<'scope> {
    lifetime: PhantomData<*mut &'scope ()>,
    arena: &'scope Arena,
}

impl Arena {
    fn index(&self) -> usize {
        self.index.get()
    }

    fn bucket_size(&self) -> usize {
        let index = self.index();
        self.buckets
            .borrow()
            .get(index)
            .map(|bucket| bucket.capacity())
            .unwrap_or(512)
    }

    fn last_bucket(&self) -> Option<Ref<Bucket>> {
        let v = self.buckets.borrow();
        let index = self.index();

        if v.is_empty() {
            None
        } else {
            Some(Ref::map(v, |bucket| &bucket[index]))
        }
    }

    fn grow(&self) {
        let len = self.bucket_size();
        self.buckets
            .borrow_mut()
            .push(Bucket::new(len * 2).unwrap());
        self.index.set(self.index() + 1);
    }
}

impl Arena {
    fn malloc<T>(&self, size: usize) -> Result<*mut T, CapacityError> {
        // TODO: ensure_capacity()
        let last = match self.last_bucket() {
            Some(last) => last,
            None => {
                self.grow();
                self.last_bucket().expect("Unreachable")
            }
        };

        match last.malloc(size) {
            Ok(ptr) => Ok(ptr),
            Err(_) => {
                drop(last);
                self.grow();
                self.last_bucket().unwrap().malloc(size)
            }
        }
    }
}

impl Arena {
    pub fn new() -> Self {
        Self {
            index: Cell::new(0),
            buckets: RefCell::new(vec![Bucket::new(512).unwrap()]),
        }
    }

    /// ```
    /// use arenalloc::{arena::Arena, collections::localbox::LocalBox};
    ///
    /// let arena = Arena::new();
    ///
    /// arena.region(|s| {
    ///     let localb = LocalBox::new(s, 10);
    ///
    ///     assert_eq!(*localb, 10);
    /// });
    ///
    /// ```
    pub fn region<F, O>(&self, f: F) -> O
    where
        F: for<'scope> FnOnce(&Scope<'scope>) -> O,
    {
        let scope = Scope {
            arena: self,
            lifetime: PhantomData,
        };
        f(&scope)
    }
}

impl Scope<'_> {
    pub fn malloc<T>(&self, size: usize) -> Result<*mut T, CapacityError> {
        self.arena.malloc(size)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_arena() {
        let mut arena = Arena::new();

        let alloc = unsafe {
            let ptr = arena.malloc::<u8>(512).unwrap();
            ptr.write(20);
            ptr
        };

        let alloc2 = unsafe {
            let ptr = arena.malloc::<u64>(1024 / 8).unwrap();
            ptr.write(1);
            ptr
        };

        let alloc3 = unsafe {
            let ptr = arena.malloc::<u8>(1).unwrap();
            ptr.write(1);
            ptr
        };
        assert_eq!(unsafe { alloc.read() }, 20);
        assert_eq!(unsafe { alloc2.read() }, 1);
        assert_eq!(unsafe { alloc3.read() }, 1);

        assert_eq!(arena.index(), 2);
        assert_eq!(arena.buckets.borrow().len(), 3);
    }
}

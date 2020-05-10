use crate::arena::Scope;

use core::{
    marker::PhantomData,
    ops::{Deref, DerefMut},
};

pub struct LocalBox<'a, 'scope, T> {
    scope: PhantomData<&'a Scope<'scope>>,
    pointer: *mut T,
}

impl<'a, 'scope, T> LocalBox<'a, 'scope, T> {
    pub fn new(scope: &'a Scope<'scope>, value: T) -> Self {
        let ptr = unsafe {
            let ptr = scope.malloc::<T>(1).expect("Allocation failed");
            ptr.write(value);
            ptr
        };

        Self {
            scope: PhantomData,
            pointer: ptr,
        }
    }
}

impl<'a, 'scope, T> Deref for LocalBox<'a, 'scope, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        unsafe { &*(self.pointer) }
    }
}

impl<'a, 'scope, T> DerefMut for LocalBox<'a, 'scope, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut *(self.pointer) }
    }
}

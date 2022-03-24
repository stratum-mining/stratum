//! Useful struct used into this crate and by crates that want to interact with this one
use std::sync::{Mutex as Mutex_, MutexGuard, PoisonError};

/// Generator of unique ids
#[derive(Debug, PartialEq)]
pub struct Id {
    state: u32,
}

impl Id {
    pub fn new() -> Self {
        Self { state: 0 }
    }
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> u32 {
        self.state += 1;
        self.state
    }
}

impl Default for Id {
    fn default() -> Self {
        Self::new()
    }
}

/// Safer Mutex wrapper
#[derive(Debug)]
pub struct Mutex<T: ?Sized>(Mutex_<T>);

impl<T> Mutex<T> {
    pub fn safe_lock<F, Ret>(&self, thunk: F) -> Result<Ret, PoisonError<MutexGuard<'_, T>>>
    where
        F: FnOnce(&mut T) -> Ret,
    {
        let mut lock = self.0.lock()?;
        let return_value = thunk(&mut *lock);
        drop(lock);
        Ok(return_value)
    }

    pub fn new(v: T) -> Self {
        Mutex(Mutex_::new(v))
    }

    pub fn to_remove(&self) -> MutexGuard<'_, T> {
        self.0.lock().unwrap()
    }
}

use alloc::vec::Vec;

use crate::{buffer_pool::BufferPool as Pool, slice::Slice, Buffer};
use rand::Rng;

#[test]
fn test() {
    assert!(true)
}

#[test]
fn pool_capicity_without_alloc() {
    let mut rng = rand::thread_rng();

    // Allocate a pool of 8 * 5 bytes
    let mut pool = Pool::new_fail_system_memory(8 * 5);

    let mut slices: Vec<Slice> = Vec::new();

    for _ in 0..8 {
        // Allocate a slice of 5 bytes in the pool
        let n1: u8 = rng.gen();
        let n2: u8 = rng.gen();
        let n3: u8 = rng.gen();
        let n4: u8 = rng.gen();
        let n5: u8 = rng.gen();
        let mut src = [n1, n2, n3, n4, n5];

        let writable = pool.get_writable(5);
        writable.copy_from_slice(&src[..]);

        let mut owned = pool.get_data_owned();
        assert_eq!(&mut src[..], owned.as_mut());

        slices.push(owned);
    }
}

#[test]
fn it_drop() {
    let mut rng = rand::thread_rng();

    // Allocate a pool of 8 * 5 bytes
    let mut pool = Pool::new_fail_system_memory(8 * 5);

    for _ in 0..100 {
        // Allocate a slice of 5 bytes in the pool
        let n1: u8 = rng.gen();
        let n2: u8 = rng.gen();
        let n3: u8 = rng.gen();
        let n4: u8 = rng.gen();
        let n5: u8 = rng.gen();
        let mut src = [n1, n2, n3, n4, n5];

        let writable = pool.get_writable(5);
        writable.copy_from_slice(&src[..]);

        let mut owned = pool.get_data_owned();
        assert_eq!(&mut src[..], owned.as_mut());
    }
}

#[test]
fn alloc_more_than_pool_capacity() {
    let mut rng = rand::thread_rng();

    // Allocate a pool of 8 * 5 bytes
    let mut pool = Pool::new(8 * 5);

    let mut slices: Vec<Slice> = Vec::new();

    for _ in 0..18 {
        // Allocate a slice of 5 bytes in the pool
        let n1: u8 = rng.gen();
        let n2: u8 = rng.gen();
        let n3: u8 = rng.gen();
        let n4: u8 = rng.gen();
        let n5: u8 = rng.gen();
        let mut src = [n1, n2, n3, n4, n5];

        let writable = pool.get_writable(5);
        writable.copy_from_slice(&src[..]);

        let mut owned = pool.get_data_owned();
        assert_eq!(&mut src[..], owned.as_mut());

        slices.push(owned);
    }
}

#[test]
#[should_panic]
fn alloc_more_than_pool_capacity_2() {
    let mut rng = rand::thread_rng();

    // Allocate a pool of 8 * 5 bytes
    let mut pool = Pool::new_fail_system_memory(8 * 5);

    let mut slices: Vec<Slice> = Vec::new();

    for _ in 0..9 {
        // Allocate a slice of 5 bytes in the pool
        let n1: u8 = rng.gen();
        let n2: u8 = rng.gen();
        let n3: u8 = rng.gen();
        let n4: u8 = rng.gen();
        let n5: u8 = rng.gen();
        let mut src = [n1, n2, n3, n4, n5];

        let writable = pool.get_writable(5);
        writable.copy_from_slice(&src[..]);

        let mut owned = pool.get_data_owned();
        assert_eq!(&mut src[..], owned.as_mut());

        slices.push(owned);
    }
}

#[test]
fn alloc_more_than_byte_capacity() {
    let mut rng = rand::thread_rng();

    // Allocate a pool of 8 * 5 bytes
    let mut pool = Pool::new(1);

    let mut slices: Vec<Slice> = Vec::new();

    for _ in 0..18 {
        // Allocate a slice of 5 bytes in the pool
        let n1: u8 = rng.gen();
        let n2: u8 = rng.gen();
        let n3: u8 = rng.gen();
        let n4: u8 = rng.gen();
        let n5: u8 = rng.gen();
        let mut src = [n1, n2, n3, n4, n5];

        let writable = pool.get_writable(5);
        writable.copy_from_slice(&src[..]);

        let mut owned = pool.get_data_owned();
        assert_eq!(&mut src[..], owned.as_mut());

        slices.push(owned);
    }
}

#[test]
#[should_panic]
fn alloc_more_than_byte_capacity_2() {
    let mut rng = rand::thread_rng();

    // Allocate a pool of 8 * 5 bytes
    let mut pool = Pool::new_fail_system_memory(1);

    let mut slices: Vec<Slice> = Vec::new();

    for _ in 0..18 {
        // Allocate a slice of 5 bytes in the pool
        let n1: u8 = rng.gen();
        let n2: u8 = rng.gen();
        let n3: u8 = rng.gen();
        let n4: u8 = rng.gen();
        let n5: u8 = rng.gen();
        let mut src = [n1, n2, n3, n4, n5];

        let writable = pool.get_writable(5);
        writable.copy_from_slice(&src[..]);

        let mut owned = pool.get_data_owned();
        assert_eq!(&mut src[..], owned.as_mut());

        slices.push(owned);
    }
}

#[test]
fn back_front_back() {
    let mut rng = rand::thread_rng();

    // Allocate a pool of 8 * 5 bytes
    let mut pool = Pool::new_fail_system_memory(8 * 5);

    let mut slices: alloc::collections::VecDeque<Slice> = alloc::collections::VecDeque::new();

    for _ in 0..8 {
        // Allocate a slice of 5 bytes in the pool
        let n1: u8 = rng.gen();
        let n2: u8 = rng.gen();
        let n3: u8 = rng.gen();
        let n4: u8 = rng.gen();
        let n5: u8 = rng.gen();
        let mut src = [n1, n2, n3, n4, n5];

        let writable = pool.get_writable(5);
        writable.copy_from_slice(&src[..]);

        let mut owned = pool.get_data_owned();
        assert_eq!(&mut src[..], owned.as_mut());

        slices.push_back(owned);
        assert!(pool.is_back_mode());
    }
    // tail 8 front 0

    // Free the first 3 slices
    slices.pop_front();
    slices.pop_front();
    slices.pop_front();

    // tail 5 front 0

    // Reallocate in front
    for _ in 0..3 {
        // Allocate a slice of 5 bytes in the pool
        let n1: u8 = rng.gen();
        let n2: u8 = rng.gen();
        let n3: u8 = rng.gen();
        let n4: u8 = rng.gen();
        let n5: u8 = rng.gen();
        let mut src = [n1, n2, n3, n4, n5];

        let writable = pool.get_writable(5);
        writable.copy_from_slice(&src[..]);

        let mut owned = pool.get_data_owned();
        assert_eq!(&mut src[..], owned.as_mut());

        slices.push_front(owned);
        assert!(pool.is_front_mode());
    }

    // tail 5 front 3

    // Free the first 3 slices in the back
    slices.pop_back();
    slices.pop_back();
    slices.pop_back();

    // tail 2 front 3

    // Reallocate in back
    for _ in 0..3 {
        // Allocate a slice of 5 bytes in the pool
        let n1: u8 = rng.gen();
        let n2: u8 = rng.gen();
        let n3: u8 = rng.gen();
        let n4: u8 = rng.gen();
        let n5: u8 = rng.gen();
        let mut src = [n1, n2, n3, n4, n5];

        let writable = pool.get_writable(5);
        writable.copy_from_slice(&src[..]);

        let mut owned = pool.get_data_owned();
        assert_eq!(&mut src[..], owned.as_mut());

        slices.push_back(owned);
    }
}
#[test]
fn back_front_alloc() {
    let mut rng = rand::thread_rng();

    // Allocate a pool of 8 * 5 bytes
    let mut pool = Pool::new(8 * 5);

    let mut slices: alloc::collections::VecDeque<Slice> = alloc::collections::VecDeque::new();

    for _ in 0..8 {
        // Allocate a slice of 5 bytes in the pool
        let n1: u8 = rng.gen();
        let n2: u8 = rng.gen();
        let n3: u8 = rng.gen();
        let n4: u8 = rng.gen();
        let n5: u8 = rng.gen();
        let mut src = [n1, n2, n3, n4, n5];

        let writable = pool.get_writable(5);
        writable.copy_from_slice(&src[..]);

        let mut owned = pool.get_data_owned();
        assert_eq!(&mut src[..], owned.as_mut());

        slices.push_back(owned);
    }

    // Free the first 3 slices
    slices.pop_front();
    slices.pop_front();
    slices.pop_front();

    // Reallocate in front
    for _ in 0..3 {
        // Allocate a slice of 5 bytes in the pool
        let n1: u8 = rng.gen();
        let n2: u8 = rng.gen();
        let n3: u8 = rng.gen();
        let n4: u8 = rng.gen();
        let n5: u8 = rng.gen();
        let mut src = [n1, n2, n3, n4, n5];

        let writable = pool.get_writable(5);
        writable.copy_from_slice(&src[..]);

        let mut owned = pool.get_data_owned();
        assert_eq!(&mut src[..], owned.as_mut());

        slices.push_front(owned);
    }

    assert!(pool.is_front_mode());

    // Allocare in alloc mode
    for _ in 0..30 {
        // Allocate a slice of 5 bytes in the pool
        let n1: u8 = rng.gen();
        let n2: u8 = rng.gen();
        let n3: u8 = rng.gen();
        let n4: u8 = rng.gen();
        let n5: u8 = rng.gen();
        let mut src = [n1, n2, n3, n4, n5];

        let writable = pool.get_writable(5);
        writable.copy_from_slice(&src[..]);

        let mut owned = pool.get_data_owned();
        assert_eq!(&mut src[..], owned.as_mut());

        slices.push_back(owned);
    }
    assert!(!pool.is_front_mode());
}

#[test]
fn back_alloc_back() {
    let mut rng = rand::thread_rng();

    // Allocate a pool of 8 * 5 bytes
    let mut pool = Pool::new(8 * 5);

    let mut slices: alloc::collections::VecDeque<Slice> = alloc::collections::VecDeque::new();

    let mut control_slices: alloc::collections::VecDeque<[u8; 5]> =
        alloc::collections::VecDeque::new();

    // Allocate 8 slices in back mode
    for _ in 0..8 {
        // Allocate a slice of 5 bytes in the pool
        let n1: u8 = rng.gen();
        let n2: u8 = rng.gen();
        let n3: u8 = rng.gen();
        let n4: u8 = rng.gen();
        let n5: u8 = rng.gen();
        let mut src = [n1, n2, n3, n4, n5];

        let writable = pool.get_writable(5);
        writable.copy_from_slice(&src[..]);

        let mut owned = pool.get_data_owned();
        assert_eq!(&mut src[..], owned.as_mut());

        slices.push_back(owned);
        control_slices.push_back(src);
    }

    // Allocate 30 with alloc
    for _ in 0..30 {
        // Allocate a slice of 5 bytes in the pool
        let n1: u8 = rng.gen();
        let n2: u8 = rng.gen();
        let n3: u8 = rng.gen();
        let n4: u8 = rng.gen();
        let n5: u8 = rng.gen();
        let mut src = [n1, n2, n3, n4, n5];

        let writable = pool.get_writable(5);
        writable.copy_from_slice(&src[..]);

        let mut owned = pool.get_data_owned();
        assert_eq!(&mut src[..], owned.as_mut());

        slices.push_front(owned);
        control_slices.push_front(src);
    }

    // Free the last 3 slices
    slices.pop_back();
    slices.pop_back();
    slices.pop_back();

    control_slices.pop_back();
    control_slices.pop_back();
    control_slices.pop_back();

    // Allocate in back mode
    for _ in 0..3 {
        // Allocate a slice of 5 bytes in the pool
        let n1: u8 = rng.gen();
        let n2: u8 = rng.gen();
        let n3: u8 = rng.gen();
        let n4: u8 = rng.gen();
        let n5: u8 = rng.gen();
        let mut src = [n1, n2, n3, n4, n5];

        let writable = pool.get_writable(5);
        writable.copy_from_slice(&src[..]);

        let mut owned = pool.get_data_owned();
        assert_eq!(&mut src[..], owned.as_mut());

        slices.push_back(owned);
        control_slices.push_back(src);
        assert!(pool.is_back_mode());
    }

    for i in 0..slices.len() {
        assert!(slices[i].as_mut() == &mut control_slices[i][..]);
    }
}

#[test]
fn back_alloc_front() {
    let mut rng = rand::thread_rng();

    // Allocate a pool of 8 * 5 bytes
    let mut pool = Pool::new(8 * 5);

    let mut slices: alloc::collections::VecDeque<Slice> = alloc::collections::VecDeque::new();

    let mut control_slices: alloc::collections::VecDeque<[u8; 5]> =
        alloc::collections::VecDeque::new();

    for _ in 0..8 {
        // Allocate a slice of 5 bytes in the pool
        let n1: u8 = rng.gen();
        let n2: u8 = rng.gen();
        let n3: u8 = rng.gen();
        let n4: u8 = rng.gen();
        let n5: u8 = rng.gen();
        let mut src = [n1, n2, n3, n4, n5];

        let writable = pool.get_writable(5);
        writable.copy_from_slice(&src[..]);

        let mut owned = pool.get_data_owned();
        assert_eq!(&mut src[..], owned.as_mut());

        slices.push_back(owned);
        control_slices.push_back(src);
    }

    // Allocate with alloc
    for _ in 0..30 {
        // Allocate a slice of 5 bytes in the pool
        let n1: u8 = rng.gen();
        let n2: u8 = rng.gen();
        let n3: u8 = rng.gen();
        let n4: u8 = rng.gen();
        let n5: u8 = rng.gen();
        let mut src = [n1, n2, n3, n4, n5];

        let writable = pool.get_writable(5);
        writable.copy_from_slice(&src[..]);

        let mut owned = pool.get_data_owned();
        assert_eq!(&mut src[..], owned.as_mut());

        slices.push_back(owned);
        control_slices.push_back(src);
    }

    // Free the first 3 slices
    slices.pop_front().unwrap();
    slices.pop_front().unwrap();
    slices.pop_front().unwrap();

    control_slices.pop_front().unwrap();
    control_slices.pop_front().unwrap();
    control_slices.pop_front().unwrap();

    // Allocare in back
    for _ in 0..3 {
        // Allocate a slice of 5 bytes in the pool
        let n1: u8 = rng.gen();
        let n2: u8 = rng.gen();
        let n3: u8 = rng.gen();
        let n4: u8 = rng.gen();
        let n5: u8 = rng.gen();
        let mut src = [n1, n2, n3, n4, n5];

        let writable = pool.get_writable(5);
        writable.copy_from_slice(&src[..]);

        let mut owned = pool.get_data_owned();
        assert_eq!(&mut src[..], owned.as_mut());

        slices.push_back(owned);
        control_slices.push_back(src);
    }

    assert!(pool.is_front_mode());

    for i in 0..slices.len() {
        assert!(slices[i].as_mut() == &mut control_slices[i][..]);
    }
}

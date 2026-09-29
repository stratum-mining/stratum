extern crate std;

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

        let writable = pool.reserve(5);
        writable.copy_from_slice(&src[..]);
        pool.commit(5);

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

        let writable = pool.reserve(5);
        writable.copy_from_slice(&src[..]);
        pool.commit(5);

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

        let writable = pool.reserve(5);
        writable.copy_from_slice(&src[..]);
        pool.commit(5);

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

        let writable = pool.reserve(5);
        writable.copy_from_slice(&src[..]);
        pool.commit(5);

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

        let writable = pool.reserve(5);
        writable.copy_from_slice(&src[..]);
        pool.commit(5);

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

        let writable = pool.reserve(5);
        writable.copy_from_slice(&src[..]);
        pool.commit(5);

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

        let writable = pool.reserve(5);
        writable.copy_from_slice(&src[..]);
        pool.commit(5);

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

        let writable = pool.reserve(5);
        writable.copy_from_slice(&src[..]);
        pool.commit(5);

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

        let writable = pool.reserve(5);
        writable.copy_from_slice(&src[..]);
        pool.commit(5);

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

        let writable = pool.reserve(5);
        writable.copy_from_slice(&src[..]);
        pool.commit(5);

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

        let writable = pool.reserve(5);
        writable.copy_from_slice(&src[..]);
        pool.commit(5);

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

        let writable = pool.reserve(5);
        writable.copy_from_slice(&src[..]);
        pool.commit(5);

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

        let writable = pool.reserve(5);
        writable.copy_from_slice(&src[..]);
        pool.commit(5);

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

        let writable = pool.reserve(5);
        writable.copy_from_slice(&src[..]);
        pool.commit(5);

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

        let writable = pool.reserve(5);
        writable.copy_from_slice(&src[..]);
        pool.commit(5);

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

        let writable = pool.reserve(5);
        writable.copy_from_slice(&src[..]);
        pool.commit(5);

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

        let writable = pool.reserve(5);
        writable.copy_from_slice(&src[..]);
        pool.commit(5);

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

        let writable = pool.reserve(5);
        writable.copy_from_slice(&src[..]);
        pool.commit(5);

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

#[test]
fn slice_released_on_another_thread_is_safe_to_reuse() {
    let mut pool = Pool::new(64);
    pool.reserve(8).copy_from_slice(&[1; 8]);
    pool.commit(8);
    let mut slice = pool.get_data_owned();

    let worker = std::thread::spawn(move || {
        slice.as_mut()[0] = 9;
        drop(slice);
    });
    while !pool.droppable() {
        std::thread::yield_now();
    }
    pool.reserve(8).copy_from_slice(&[2; 8]);
    pool.commit(8);

    worker.join().unwrap();
}

#[test]
fn pool_backed_slice_reports_its_length() {
    let mut pool = Pool::new(64);
    pool.reserve(8).copy_from_slice(&[1; 8]);
    pool.commit(8);
    let slice = pool.get_data_owned();

    assert_eq!(slice.len(), 8);
    assert_eq!(slice.len(), slice.as_ref().len());
}

#[test]
fn repeated_shared_views_of_a_slice_coexist() {
    let mut pool = Pool::new(64);
    pool.reserve(8).copy_from_slice(&[1; 8]);
    pool.commit(8);
    let slice = pool.get_data_owned();

    let first = slice.as_ref();
    let second = slice.as_ref();
    assert_eq!(first, second);
}

#[test]
fn cloned_slice_keeps_its_bytes_after_the_original_is_released() {
    let mut pool = Pool::new(64);
    pool.reserve(8).copy_from_slice(&[1; 8]);
    pool.commit(8);
    let original = pool.get_data_owned();
    let copy = original.clone();
    drop(original);

    pool.reserve(8).copy_from_slice(&[2; 8]);
    pool.commit(8);
    let _reused = pool.get_data_owned();

    assert_eq!(copy.as_ref(), &[1; 8]);
}

#[test]
fn empty_frame_does_not_take_a_slot() {
    let mut pool = Pool::new(64);
    let empty = pool.get_data_owned();

    assert!(empty.is_empty());
    assert!(pool.droppable());
}

#[test]
fn slice_view_survives_a_later_allocation() {
    let mut pool = Pool::new(64);
    pool.reserve(8).copy_from_slice(&[1; 8]);
    pool.commit(8);
    let mut first = pool.get_data_owned();
    let view = first.as_mut();

    pool.reserve(8).copy_from_slice(&[2; 8]);
    pool.commit(8);
    let _second = pool.get_data_owned();

    view[0] = 9;
    assert_eq!(first.as_ref()[0], 9);
}

#[test]
fn formatting_the_pool_does_not_race_a_live_slice() {
    let mut pool = Pool::new(64);
    pool.reserve(8).copy_from_slice(&[1; 8]);
    pool.commit(8);
    let mut live = pool.get_data_owned();

    let worker = std::thread::spawn(move || {
        live.as_mut()[0] = 9;
        live
    });
    let _ = alloc::format!("{pool:?}");

    drop(worker.join().unwrap());
}

#[test]
fn slice_outlives_its_pool() {
    let mut pool = Pool::new(64);
    pool.reserve(8).copy_from_slice(&[1; 8]);
    pool.commit(8);
    let slice = pool.get_data_owned();
    drop(pool);

    assert_eq!(slice.as_ref(), &[1; 8]);
}

#[test]
fn front_and_back_cycle_keeps_the_live_tail_count() {
    let mut pool = Pool::new_fail_system_memory(8);
    let mut back = Vec::new();
    for value in 0_u8..8 {
        pool.reserve(1)[0] = value;
        pool.commit(1);
        back.push(pool.get_data_owned());
    }
    back.drain(..3);

    let mut first_front = Vec::new();
    for value in 8_u8..11 {
        pool.reserve(1)[0] = value;
        pool.commit(1);
        first_front.push(pool.get_data_owned());
    }
    assert!(pool.is_front_mode());

    drop(back.pop());
    pool.reserve(1)[0] = 11;
    pool.commit(1);
    back.push(pool.get_data_owned());
    assert!(pool.is_back_mode());

    drop(first_front);
    back.drain(..2);

    let mut second_front = Vec::new();
    for value in 12_u8..17 {
        pool.reserve(1)[0] = value;
        pool.commit(1);
        second_front.push(pool.get_data_owned());
    }
    assert!(pool.is_front_mode());

    // Slot 7 is free again, so the pool must reuse it instead of allocating.
    drop(back.pop());
    pool.reserve(1)[0] = 17;
    pool.commit(1);
    let reused = pool.get_data_owned();

    assert!(pool.is_back_mode());
    assert_eq!(reused.as_ref(), &[17]);
    for (i, slice) in back.iter().enumerate() {
        assert_eq!(slice.as_ref(), &[i as u8 + 5]);
    }
    for (i, slice) in second_front.iter().enumerate() {
        assert_eq!(slice.as_ref(), &[i as u8 + 12]);
    }
}

#[test]
fn pool_leaves_alloc_mode_after_a_failed_head_clear() {
    let mut pool = Pool::new(8);
    let mut back = Vec::new();
    for value in 0_u8..8 {
        pool.reserve(1)[0] = value;
        pool.commit(1);
        back.push(pool.get_data_owned());
    }
    back.drain(..3);

    let mut front = Vec::new();
    for value in 8_u8..11 {
        pool.reserve(1)[0] = value;
        pool.commit(1);
        front.push(pool.get_data_owned());
    }
    assert!(pool.is_front_mode());

    drop(back.pop());
    pool.reserve(1)[0] = 11;
    pool.commit(1);
    let replacement = pool.get_data_owned();
    assert!(pool.is_back_mode());

    // Every slot is live, so this frame falls back to system memory.
    pool.reserve(1)[0] = 12;
    pool.commit(1);
    drop(pool.get_data_owned());
    assert!(pool.is_alloc_mode());

    // The last pooled slot is free again, so the pool must return to back mode and reuse it.
    drop(replacement);
    pool.reserve(1)[0] = 13;
    pool.commit(1);
    let reused = pool.get_data_owned();

    assert!(pool.is_back_mode());
    assert_eq!(reused.as_ref(), &[13]);
    assert_eq!(front[0].as_ref(), &[8]);
    assert_eq!(back[0].as_ref(), &[3]);
}

#[test]
fn a_live_front_slot_past_the_free_prefix_keeps_the_back_usable() {
    let mut pool = Pool::new(80);
    let mut back = Vec::new();
    for value in 0_u8..8 {
        pool.reserve(10).fill(value);
        pool.commit(10);
        back.push(Some(pool.get_data_owned()));
    }
    for slot in &mut back[..3] {
        *slot = None;
    }

    let mut front = Vec::new();
    for value in 8_u8..11 {
        pool.reserve(10).fill(value);
        pool.commit(10);
        front.push(Some(pool.get_data_owned()));
    }
    assert!(pool.is_front_mode());
    front[0] = None;
    front[2] = None;

    // Nothing in the pool fits 11 bytes, so this frame falls back to system memory.
    pool.reserve(11).fill(11);
    pool.commit(11);
    drop(pool.get_data_owned());
    assert!(pool.is_alloc_mode());

    // Only slot 0 is free before the live front slot 1, so the front shrinks to that slot.
    pool.reserve(10).fill(12);
    pool.commit(10);
    let narrowed_front = pool.get_data_owned();
    assert!(pool.is_front_mode());

    // The back tail is free again, so the pool must reuse it while front slot 1 is still live.
    for slot in &mut back[5..] {
        *slot = None;
    }
    pool.reserve(10).fill(13);
    pool.commit(10);
    let reused = pool.get_data_owned();

    assert!(pool.is_back_mode());
    assert_eq!(reused.as_ref(), &[13; 10]);
    assert_eq!(narrowed_front.as_ref(), &[12; 10]);
    assert_eq!(front[1].as_ref().unwrap().as_ref(), &[9; 10]);
    for (value, slot) in (3_u8..5).zip(&back[3..5]) {
        assert_eq!(slot.as_ref().unwrap().as_ref(), &[value; 10]);
    }
}

#[test]
fn switching_front_to_back_does_not_overwrite_a_live_slice() {
    let mut pool = Pool::new(80);
    let mut back = Vec::new();
    for _ in 0..8 {
        pool.reserve(10).fill(0x11);
        pool.commit(10);
        back.push(pool.get_data_owned());
    }
    back.drain(..3);

    pool.reserve(1).fill(0x22);
    pool.commit(1);
    let first_front = pool.get_data_owned();
    pool.reserve(1).fill(0x33);
    pool.commit(1);
    let second_front = pool.get_data_owned();
    pool.reserve(1).fill(0x44);
    pool.commit(1);
    let third_front = pool.get_data_owned();
    assert!(pool.is_front_mode());

    // Front slot 1 is reused for a larger slice, while freed slot 2 still records its old
    // extent.
    drop(second_front);
    drop(third_front);
    pool.reserve(10).fill(0x55);
    pool.commit(10);
    let live_front = pool.get_data_owned();

    // Every back slot is free, and 20 bytes don't fit before the old boundary, so the pool
    // switches back to the back.
    drop(back);
    pool.reserve(20).fill(0x66);
    pool.commit(20);
    let new_back = pool.get_data_owned();

    assert_eq!(first_front.as_ref(), &[0x22]);
    assert_eq!(live_front.as_ref(), &[0x55; 10]);
    assert_eq!(new_back.as_ref(), &[0x66; 20]);
}

#[test]
fn freeing_every_front_slot_keeps_the_back_slices_live() {
    let mut pool = Pool::new(80);
    let mut back = Vec::new();
    for value in 0_u8..8 {
        pool.reserve(10).fill(value);
        pool.commit(10);
        back.push(pool.get_data_owned());
    }
    back.remove(0);

    pool.reserve(1).fill(0x11);
    pool.commit(1);
    let front = pool.get_data_owned();
    assert!(pool.is_front_mode());
    drop(front);

    pool.reserve(1).fill(0x22);
    pool.commit(1);
    let next = pool.get_data_owned();

    assert_eq!(next.as_ref(), &[0x22]);
    for (value, slice) in (1_u8..8).zip(&back) {
        assert_eq!(slice.as_ref(), &[value; 10]);
    }
}

#[test]
fn random_slice_lifetimes_match_a_model_of_the_pool() {
    use rand::{rngs::StdRng, SeedableRng};

    let (seeds, steps) = if cfg!(miri) { (1, 100) } else { (50, 2_000) };
    for capacity in [0_usize, 1, 7, 40, 80, 1000] {
        for seed in 0..seeds {
            let mut rng = StdRng::seed_from_u64(seed);
            let mut pool = Pool::new(capacity);
            let mut live: Vec<(Slice, Vec<u8>)> = Vec::new();

            for step in 0..steps {
                if live.is_empty() || (live.len() < 10 && rng.gen_bool(0.5)) {
                    let mut expected = Vec::new();
                    for _ in 0..rng.gen_range(1..=2) {
                        let len = match rng.gen_range(0..3) {
                            0 => 1,
                            1 => rng.gen_range(1..=capacity / 8 + 1),
                            _ => rng.gen_range(1..=capacity / 3 + 1),
                        };
                        let byte: u8 = rng.gen();
                        pool.reserve(len).fill(byte);
                        pool.commit(len);
                        expected.resize(expected.len() + len, byte);
                    }
                    live.push((pool.get_data_owned(), expected));
                } else {
                    let index = match rng.gen_range(0..3) {
                        0 => 0,
                        1 => live.len() - 1,
                        _ => rng.gen_range(0..live.len()),
                    };
                    live.remove(index);
                }

                for (slice, expected) in &live {
                    assert_eq!(
                        slice.as_ref(),
                        &expected[..],
                        "capacity {capacity}, seed {seed}, step {step}"
                    );
                }
                let mut ranges: Vec<(usize, usize)> = live
                    .iter()
                    .map(|(slice, _)| slice.as_ref())
                    .map(|bytes| (bytes.as_ptr() as usize, bytes.len()))
                    .collect();
                ranges.sort_unstable();
                for pair in ranges.windows(2) {
                    assert!(
                        pair[0].0 + pair[0].1 <= pair[1].0,
                        "capacity {capacity}, seed {seed}, step {step}"
                    );
                }
            }
        }
    }
}

#[test]
fn a_repeated_or_partly_used_reservation_counts_only_committed_bytes() {
    for capacity in [0, 64] {
        let mut pool = Pool::new(capacity);
        pool.reserve(8).fill(1);
        pool.reserve(8).fill(2);
        assert_eq!(Buffer::len(&pool), 0);

        pool.reserve(8)[..3].fill(3);
        pool.commit(3);

        assert_eq!(Buffer::len(&pool), 3);
        assert_eq!(pool.get_data_owned().as_ref(), &[3; 3]);
    }
}

#[test]
#[should_panic]
fn committing_more_than_the_pool_reserved_panics() {
    let mut pool = Pool::new(64);
    pool.reserve(2);
    pool.commit(3);
}

#[test]
#[should_panic]
fn committing_more_than_system_memory_reserved_panics() {
    let mut memory = crate::BufferFromSystemMemory::new(0);
    memory.reserve(2);
    memory.commit(3);
}

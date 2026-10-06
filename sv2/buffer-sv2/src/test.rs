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

#[test]
fn slice_released_on_another_thread_is_safe_to_reuse() {
    let mut pool = Pool::new(64);
    pool.get_writable(8).copy_from_slice(&[1; 8]);
    let mut slice = pool.get_data_owned();

    let worker = std::thread::spawn(move || {
        slice.as_mut()[0] = 9;
        drop(slice);
    });
    while !pool.droppable() {
        std::thread::yield_now();
    }
    pool.get_writable(8).copy_from_slice(&[2; 8]);

    worker.join().unwrap();
}

#[test]
fn pool_backed_slice_reports_its_length() {
    let mut pool = Pool::new(64);
    pool.get_writable(8).copy_from_slice(&[1; 8]);
    let slice = pool.get_data_owned();

    assert_eq!(slice.len(), 8);
    assert_eq!(slice.len(), slice.as_ref().len());
}

#[test]
fn repeated_shared_views_of_a_slice_coexist() {
    let mut pool = Pool::new(64);
    pool.get_writable(8).copy_from_slice(&[1; 8]);
    let slice = pool.get_data_owned();

    let first = slice.as_ref();
    let second = slice.as_ref();
    assert_eq!(first, second);
}

#[test]
fn cloned_slice_keeps_its_bytes_after_the_original_is_released() {
    let mut pool = Pool::new(64);
    pool.get_writable(8).copy_from_slice(&[1; 8]);
    let original = pool.get_data_owned();
    let copy = original.clone();
    drop(original);

    pool.get_writable(8).copy_from_slice(&[2; 8]);
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
    pool.get_writable(8).copy_from_slice(&[1; 8]);
    let mut first = pool.get_data_owned();
    let view = first.as_mut();

    pool.get_writable(8).copy_from_slice(&[2; 8]);
    let _second = pool.get_data_owned();

    view[0] = 9;
    assert_eq!(first.as_ref()[0], 9);
}

#[test]
fn formatting_the_pool_does_not_race_a_live_slice() {
    let mut pool = Pool::new(64);
    pool.get_writable(8).copy_from_slice(&[1; 8]);
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
    pool.get_writable(8).copy_from_slice(&[1; 8]);
    let slice = pool.get_data_owned();
    drop(pool);

    assert_eq!(slice.as_ref(), &[1; 8]);
}

#[test]
fn front_and_back_cycle_keeps_the_live_tail_count() {
    let mut pool = Pool::new_fail_system_memory(8);
    let mut back = Vec::new();
    for value in 0_u8..8 {
        pool.get_writable(1)[0] = value;
        back.push(pool.get_data_owned());
    }
    back.drain(..3);

    let mut first_front = Vec::new();
    for value in 8_u8..11 {
        pool.get_writable(1)[0] = value;
        first_front.push(pool.get_data_owned());
    }
    assert!(pool.is_front_mode());

    drop(back.pop());
    pool.get_writable(1)[0] = 11;
    back.push(pool.get_data_owned());
    assert!(pool.is_back_mode());

    drop(first_front);
    back.drain(..2);

    let mut second_front = Vec::new();
    for value in 12_u8..17 {
        pool.get_writable(1)[0] = value;
        second_front.push(pool.get_data_owned());
    }
    assert!(pool.is_front_mode());

    // Slot 7 is free again, so the pool must reuse it instead of allocating.
    drop(back.pop());
    pool.get_writable(1)[0] = 17;
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
        pool.get_writable(1)[0] = value;
        back.push(pool.get_data_owned());
    }
    back.drain(..3);

    let mut front = Vec::new();
    for value in 8_u8..11 {
        pool.get_writable(1)[0] = value;
        front.push(pool.get_data_owned());
    }
    assert!(pool.is_front_mode());

    drop(back.pop());
    pool.get_writable(1)[0] = 11;
    let replacement = pool.get_data_owned();
    assert!(pool.is_back_mode());

    // Every slot is live, so this frame falls back to system memory.
    pool.get_writable(1)[0] = 12;
    drop(pool.get_data_owned());
    assert!(pool.is_alloc_mode());

    // The last pooled slot is free again, so the pool must return to back mode and reuse it.
    drop(replacement);
    pool.get_writable(1)[0] = 13;
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
        pool.get_writable(10).fill(value);
        back.push(Some(pool.get_data_owned()));
    }
    for slot in &mut back[..3] {
        *slot = None;
    }

    let mut front = Vec::new();
    for value in 8_u8..11 {
        pool.get_writable(10).fill(value);
        front.push(Some(pool.get_data_owned()));
    }
    assert!(pool.is_front_mode());
    front[0] = None;
    front[2] = None;

    // Nothing in the pool fits 11 bytes, so this frame falls back to system memory.
    pool.get_writable(11).fill(11);
    drop(pool.get_data_owned());
    assert!(pool.is_alloc_mode());

    // Only slot 0 is free before the live front slot 1, so the front shrinks to that slot.
    pool.get_writable(10).fill(12);
    let narrowed_front = pool.get_data_owned();
    assert!(pool.is_front_mode());

    // The back tail is free again, so the pool must reuse it while front slot 1 is still live.
    for slot in &mut back[5..] {
        *slot = None;
    }
    pool.get_writable(10).fill(13);
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
        pool.get_writable(10).fill(0x11);
        back.push(pool.get_data_owned());
    }
    back.drain(..3);

    pool.get_writable(1).fill(0x22);
    let first_front = pool.get_data_owned();
    pool.get_writable(1).fill(0x33);
    let second_front = pool.get_data_owned();
    pool.get_writable(1).fill(0x44);
    let third_front = pool.get_data_owned();
    assert!(pool.is_front_mode());

    // Front slot 1 is reused for a larger slice, while freed slot 2 still records its old
    // extent.
    drop(second_front);
    drop(third_front);
    pool.get_writable(10).fill(0x55);
    let live_front = pool.get_data_owned();

    // Every back slot is free, and 20 bytes don't fit before the old boundary, so the pool
    // switches back to the back.
    drop(back);
    pool.get_writable(20).fill(0x66);
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
        pool.get_writable(10).fill(value);
        back.push(pool.get_data_owned());
    }
    back.remove(0);

    pool.get_writable(1).fill(0x11);
    let front = pool.get_data_owned();
    assert!(pool.is_front_mode());
    drop(front);

    pool.get_writable(1).fill(0x22);
    let next = pool.get_data_owned();

    assert_eq!(next.as_ref(), &[0x22]);
    for (value, slice) in (1_u8..8).zip(&back) {
        assert_eq!(slice.as_ref(), &[value; 10]);
    }
}

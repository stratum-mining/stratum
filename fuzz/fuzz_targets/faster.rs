#![no_main]
use arbitrary::Arbitrary;
use buffer_sv2::BufferPool as Pool;
use buffer_sv2::{Buffer, Slice};
use libfuzzer_sys::fuzz_target;
use std::fs::File;
use std::io::Read;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use threadpool::ThreadPool;
#[macro_use]
extern crate lazy_static;

// A global ThreadPool is used cause after the creation of ~4M threads asan report an error
lazy_static! {
    static ref T_POOL: Arc<Mutex<ThreadPool>> = Arc::new(Mutex::new(ThreadPool::new(1000)));
}

#[inline(always)]
fn add_random_bytes_test(bytes: usize, buffer: &mut impl Buffer, input: &[u8]) -> Vec<u8> {
    let rounds = bytes / 10;

    let mut v = Vec::with_capacity(bytes);

    for _ in 0..rounds {
        let writable: &mut [u8] = buffer.get_writable(input.len());
        if writable.is_empty() {
            return v;
        }
        writable.copy_from_slice(input);
        v.extend_from_slice(input);
    }
    v
}

#[inline(always)]
fn keep_slice_test(mut slice: Slice, mut control: Vec<u8>, ms: u64, t_pool: &mut ThreadPool) {
    t_pool.execute(move || {
        #[allow(deprecated)]
        std::thread::sleep(Duration::from_micros(ms));
        let control: &mut [u8] = control.as_mut();
        if slice.as_mut() != control {
            println!("{:?}", slice);
            let slice = slice.as_mut();
            println!(
                "{:?} {} {:?} {}",
                &slice[..5],
                slice.len(),
                &control[..5],
                control.len()
            );
            assert!(slice == control);
        }
    })
}

const MESSAGE_LENGTH: usize = 2_usize.pow(14);

#[derive(Arbitrary, Debug)]
struct Input {
    micros: Vec<u16>,
    capacity: u32,
}

fuzz_target!(|data: Input| {
    let mut t_pool = T_POOL.lock().unwrap();
    let mut data = data;
    if data.micros.is_empty() {
        data.micros.push(1);
    }

    let mut file = File::open("random").unwrap();
    let mut input: Vec<u8> = Vec::new();
    file.read_to_end(&mut input).unwrap();

    let mut pool = Pool::new(data.capacity as usize);

    for i in 0..1000 {
        let ms = data.micros[(data.micros.len() - 1) % (i + 1)] as u64;
        let control = add_random_bytes_test(MESSAGE_LENGTH, &mut pool, &input[i..i + 10]);
        keep_slice_test(pool.get_data_owned(), control, ms, &mut t_pool);
    }

    // Otherway asan will complain
    t_pool.join();
});

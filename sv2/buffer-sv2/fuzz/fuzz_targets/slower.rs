#![no_main]
use affinity::{get_core_num, set_thread_affinity};
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
fn add_random_bytes_test(
    m_len: usize,
    buffer: &mut impl Buffer,
    input: &[u8],
    w_lens: Vec<u16>,
) -> Vec<u8> {
    let mut v = Vec::with_capacity(m_len);
    let mut i: usize = 0;
    let mut written = 0;
    let now = std::time::SystemTime::now();

    while written < m_len {
        let w_len = w_lens[(w_lens.len() - 1) % (i + 1)] as usize;
        if written + w_len >= input.len() {
            break;
        };

        let writable: &mut [u8] = buffer.get_writable(w_len);
        writable.copy_from_slice(&input[written..written + w_len]);
        v.extend_from_slice(&input[written..written + w_len]);

        written += w_len;
        i += 1;

        let elapsed = now.elapsed().unwrap().as_millis();
        if elapsed > 500 {
            println!();
            println!("The input is taking more than half seconds:");
            break;
        }
    }

    v
}

#[inline(always)]
fn keep_slice_test(
    mut slice: Slice,
    mut control: Vec<u8>,
    ms: u64,
    core: usize,
    t_pool: &mut ThreadPool,
) {
    t_pool.execute(move || {
        set_thread_affinity(&[core][..]).unwrap();
        #[allow(deprecated)]
        std::thread::sleep(Duration::from_micros(ms));
        let control: &mut [u8] = control.as_mut();
        if slice.as_mut() != control {
            println!("{:?} {:?}", &slice.as_mut()[..20], &control[..20]);
            assert!(slice.as_mut() == control);
        }
    });
}

const MESSAGE_LENGTH: u32 = 2_u32.pow(14); // - rng.gen_range(0..12000);
const MAX_MESSAGE_LEN: usize = 9_000_000;

#[derive(Arbitrary, Debug)]
struct Input {
    micros: Vec<u16>,
    rounds: (u8, u8, u8),
    m_len: Vec<u32>,
    w_len: Vec<u16>,
    capacity: u32,
}

fuzz_target!(|data: Input| {
    let mut t_pool = T_POOL.lock().unwrap();
    let mut file = File::open("random").unwrap();
    let mut input: Vec<u8> = Vec::new();
    file.read_to_end(&mut input).unwrap();

    let mut data = data;

    if data.micros.is_empty() {
        data.micros.push(1);
    }

    if data.m_len.is_empty() {
        data.m_len.push(MESSAGE_LENGTH)
    }

    if data.w_len.is_empty() {
        data.w_len.push(10)
    }

    let rounds = data.rounds.0 as usize + data.rounds.1 as usize + data.rounds.2 as usize;

    let mut pool = Pool::new(data.capacity as usize);

    let cores: Vec<usize> = (0..get_core_num()).collect();

    let now = std::time::SystemTime::now();

    for i in 0..rounds {
        if i >= input.len() {
            break;
        }

        let message_len = data.m_len[(data.m_len.len() - 1) % (i + 1)] as usize;

        // Too big messages take too much time to be written
        if message_len > MAX_MESSAGE_LEN {
            continue;
        }

        let ms = data.micros[(data.micros.len() - 1) % (i + 1)] as u64;

        let core = cores[(cores.len() - 1) % (i + 1)];

        let control =
            add_random_bytes_test(message_len, &mut pool, &input[i..], data.w_len.clone());
        keep_slice_test(pool.get_data_owned(), control, ms, core, &mut t_pool);

        // If an input is taking more than 1'' just print the input and pass to the next one
        let elapsed = now.elapsed().unwrap().as_secs();
        if elapsed > 1 {
            println!();
            println!("The input is taking more than 1 seconds:");
            break;
        }
    }

    // Otherway asan will complain
    t_pool.join();
});

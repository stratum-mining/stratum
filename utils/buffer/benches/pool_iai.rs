use buffer_sv2::{Buffer, BufferFromSystemMemory as BufferFromMemory, BufferPool as Pool};
use core::time::Duration;
use rand::Rng;

mod control_struct;
use control_struct::{add_random_bytes, bench_no_thread, keep_slice, MaxEfficeincy, PPool, DATA};

fn with_pool() {
    let data = DATA;
    let capacity: usize = 2_usize.pow(16) * 5;
    let pool = Pool::new(capacity);
    bench_no_thread(pool, &data[..]);
}

fn without_pool() {
    let data = DATA;
    let buffer = BufferFromMemory::new(0);
    bench_no_thread(buffer, &data[..]);
}

fn with_contro_struct() {
    let data = DATA;
    let capacity: usize = 2_usize.pow(16) * 5;
    let pool = PPool::new(capacity);
    bench_no_thread(pool, &data[..]);
}

fn with_contro_struct_max_e() {
    let data = DATA;
    let capacity: usize = 2_usize.pow(16) * 5;
    let pool = MaxEfficeincy::new(capacity);
    bench_no_thread(pool, &data[..]);
}

fn with_pool_trreaded_1() {
    let data = DATA;
    let capacity: usize = 2_usize.pow(16) * 5;
    let mut pool = Pool::new(capacity);
    let mut rng = rand::thread_rng();
    let d = Duration::from_micros(10);
    for i in 0..1000 {
        let message_length = 2_usize.pow(14) - rng.gen_range(0..12000);
        add_random_bytes(message_length, &mut pool, &data[i..]);
        keep_slice(pool.get_data_owned(), d);
        add_random_bytes(message_length, &mut pool, &data[i..]);
        keep_slice(pool.get_data_owned(), d);
    }
}

// MAX two slice x time
fn with_pool_trreaded_2() {
    let data = DATA;
    let capacity: usize = 2_usize.pow(16) * 5;
    let mut pool = Pool::new(capacity);
    let mut rng = rand::thread_rng();
    let d = Duration::from_nanos(10);
    for i in 0..1000 {
        let message_length = 2_usize.pow(14) - rng.gen_range(0..12000);
        add_random_bytes(message_length, &mut pool, &data[i..]);
        keep_slice(pool.get_data_owned(), d);
        add_random_bytes(message_length, &mut pool, &data[i..]);
        keep_slice(pool.get_data_owned(), d);
        std::thread::sleep(d);
    }
}

fn without_pool_threaded_1() {
    let data = DATA;
    let capacity: usize = 2_usize.pow(16) * 5;
    let mut pool = Pool::new(capacity);
    let mut buffer = BufferFromMemory::new(0);
    let mut rng = rand::thread_rng();
    let d = Duration::from_micros(10);
    for i in 0..1000 {
        let message_length = 2_usize.pow(16) - rng.gen_range(0..12000);
        add_random_bytes(message_length, &mut pool, &data[i..]);
        keep_slice(buffer.get_data_owned(), d);
        add_random_bytes(message_length, &mut pool, &data[i..]);
        keep_slice(buffer.get_data_owned(), d);
    }
}

// MAX two slice x time
fn without_pool_threaded_2() {
    let data = DATA;
    let capacity: usize = 2_usize.pow(16) * 5;
    let mut pool = Pool::new(capacity);
    let mut buffer = BufferFromMemory::new(0);
    let mut rng = rand::thread_rng();
    let d = Duration::from_nanos(10);
    for i in 0..1000 {
        let message_length = 2_usize.pow(16) - rng.gen_range(0..12000);
        add_random_bytes(message_length, &mut pool, &data[i..]);
        keep_slice(buffer.get_data_owned(), d);
        add_random_bytes(message_length, &mut pool, &data[i..]);
        keep_slice(buffer.get_data_owned(), d);
        std::thread::sleep(d);
    }
}

fn with_control_threaded() {
    let data = DATA;
    let capacity: usize = 2_usize.pow(16) * 5;
    let mut pool = PPool::new(capacity);
    let mut rng = rand::thread_rng();
    let d = Duration::from_micros(10);
    for i in 0..1000 {
        let message_length = 2_usize.pow(14) - rng.gen_range(0..12000);
        add_random_bytes(message_length, &mut pool, &data[i..]);
        keep_slice(pool.get_data_owned(), d);
        add_random_bytes(message_length, &mut pool, &data[i..]);
        keep_slice(pool.get_data_owned(), d);
    }
}

fn with_control_threaded_2() {
    let data = DATA;
    let capacity: usize = 2_usize.pow(16) * 5;
    let mut pool = PPool::new(capacity);
    let mut rng = rand::thread_rng();
    let d = Duration::from_nanos(10);
    for i in 0..1000 {
        let message_length = 2_usize.pow(14) - rng.gen_range(0..12000);
        add_random_bytes(message_length, &mut pool, &data[i..]);
        keep_slice(pool.get_data_owned(), d);
        add_random_bytes(message_length, &mut pool, &data[i..]);
        keep_slice(pool.get_data_owned(), d);
        std::thread::sleep(d);
    }
}

fn with_control_max_threaded() {
    let data = DATA;
    let capacity: usize = 2_usize.pow(16) * 5;
    let mut pool = MaxEfficeincy::new(capacity);
    let mut rng = rand::thread_rng();
    let d = Duration::from_micros(10);
    for i in 0..1000 {
        let message_length = 2_usize.pow(14) - rng.gen_range(0..12000);
        add_random_bytes(message_length, &mut pool, &data[i..]);
        keep_slice(pool.get_data_owned(), d);
        add_random_bytes(message_length, &mut pool, &data[i..]);
        keep_slice(pool.get_data_owned(), d);
    }
}

fn with_control_max_threaded_2() {
    let data = DATA;
    let capacity: usize = 2_usize.pow(16) * 5;
    let mut pool = MaxEfficeincy::new(capacity);
    let mut rng = rand::thread_rng();
    let d = Duration::from_nanos(10);
    for i in 0..1000 {
        let message_length = 2_usize.pow(14) - rng.gen_range(0..12000);
        add_random_bytes(message_length, &mut pool, &data[i..]);
        keep_slice(pool.get_data_owned(), d);
        add_random_bytes(message_length, &mut pool, &data[i..]);
        keep_slice(pool.get_data_owned(), d);
        std::thread::sleep(d);
    }
}

iai::main!(
    with_pool,
    without_pool,
    with_contro_struct,
    with_contro_struct_max_e,
    with_pool_trreaded_1,
    without_pool_threaded_1,
    with_control_threaded,
    with_control_max_threaded,
    with_pool_trreaded_2,
    without_pool_threaded_2,
    with_control_threaded_2,
    with_control_max_threaded_2,
);

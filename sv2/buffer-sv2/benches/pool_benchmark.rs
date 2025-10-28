use core::sync::atomic::Ordering;
use criterion::{criterion_group, criterion_main, Criterion};

use buffer_sv2::{Buffer, BufferFromSystemMemory as BufferFromMemory, BufferPool as Pool, Slice};
use core::time::Duration;
use rand::Rng;

mod control_struct;
use control_struct::{
    add_random_bytes, bench_no_thread, keep_slice, Load, MaxESlice, MaxEfficeincy, PPool, SSlice,
    DATA,
};

fn with_pool(data: &[u8]) {
    let capacity: usize = 2_usize.pow(16) * 5;
    let pool = Pool::new(capacity);
    bench_no_thread(pool, &data[..]);
}

fn without_pool(data: &[u8]) {
    let buffer = BufferFromMemory::new(0);
    bench_no_thread(buffer, &data[..]);
}

fn with_contro_struct(data: &[u8]) {
    let capacity: usize = 2_usize.pow(16) * 5;
    let pool = PPool::new(capacity);
    bench_no_thread(pool, &data[..]);
}

fn with_contro_struct_max_e(data: &[u8]) {
    let capacity: usize = 2_usize.pow(16) * 5;
    let pool = MaxEfficeincy::new(capacity);
    bench_no_thread(pool, &data[..]);
}

fn with_pool_trreaded_1(data: &[u8]) {
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
fn with_pool_trreaded_2(data: &[u8]) {
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

fn without_pool_threaded_1(data: &[u8]) {
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
fn without_pool_threaded_2(data: &[u8]) {
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

fn with_control_threaded(data: &[u8]) {
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

fn with_control_threaded_2(data: &[u8]) {
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

fn with_control_max_threaded(data: &[u8]) {
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

fn with_control_max_threaded_2(data: &[u8]) {
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

#[inline(always)]
fn keep_slice_test(mut slice: Slice, mut control: Vec<u8>) {
    std::thread::spawn(move || {
        let mut rng = rand::thread_rng();
        let ms = rng.gen_range(0..5);
        std::thread::sleep(core::time::Duration::from_millis(ms));
        let control: &mut [u8] = control.as_mut();
        if slice.as_mut() != control {
            panic!()
        }
    });
}

#[inline(always)]
fn add_random_bytes_test(
    m_len: usize,
    buffer: &mut Pool<BufferFromMemory>,
    input: &[u8],
    w_lens: Vec<u16>,
) -> Vec<u8> {
    let mut v = Vec::with_capacity(m_len);
    let mut i: usize = 0;
    let mut written = 0;
    //println!("START {}", buffer.is_alloc_mode());

    while written < m_len {
        //println!("{}", written);
        let w_len = w_lens[(w_lens.len() - 1) % (i + 1)] as usize;
        if written + w_len >= input.len() {
            break;
        };

        let writable: &mut [u8] = buffer.get_writable(w_len).as_mut();
        writable.copy_from_slice(&input[written..written + w_len]);
        v.extend_from_slice(&input[written..written + w_len]);

        assert!(
            &writable[..] == &input[written..written + w_len]
                && &writable[..] == &v[written..written + w_len]
        );

        written += w_len;
        i += 1;
    }
    //println!("END {}", buffer.is_back_mode());
    let i = buffer.get_data_by_ref(written);
    assert!(&v[..] == i);
    v
}

#[inline(always)]
fn keep_slice_test_p(mut slice: Slice, mut control: Vec<u8>, ms: u64) {
    let i = slice.index;
    std::thread::spawn(move || {
        std::thread::sleep(core::time::Duration::from_micros(ms));
        let control: &mut [u8] = control.as_mut();
        //if slice.index == 2 && slice.owned.is_none() {
        //    println!("{:#?}", slice);
        //}
        if slice.as_mut() != control {
            println!("{:?} {:?}", &slice.as_mut()[..20], &control[..20]);
            std::process::exit(9)
        }
    });
}

const MESSAGE_LENGTH: usize = 2_usize.pow(14);

fn with_pool_trreaded_test_1(data: &[u8]) {
    let w_lens = vec![10];
    let micros = vec![64511, 9471];

    let mut pool = Pool::new(130816);

    for i in 0..1000 {
        let ms = micros[(micros.len() - 1) % (i + 1)] as u64;
        let control = add_random_bytes_test(MESSAGE_LENGTH, &mut pool, &data[i..], w_lens.clone());
        let mut slice = pool.get_data_owned();
        if slice.as_mut() != control {
            println!("{:?} {:?}", &slice.as_mut()[..20], &control[..20]);
            std::process::exit(9)
        }
        keep_slice_test_p(slice, control, ms);
    }
}

fn with_pool_trreaded_test_2(data: &[u8]) {
    let capacity: usize = 2_usize.pow(16) * 10;
    let w_lens = vec![10];
    let micros = vec![1];

    let mut pool = Pool::new(capacity);

    for i in 0..1000 {
        let ms = micros[(micros.len() - 1) % (i + 1)] as u64;
        let control = add_random_bytes_test(MESSAGE_LENGTH, &mut pool, &data[i..], w_lens.clone());
        let mut slice = pool.get_data_owned();
        if slice.as_mut() != control {
            println!("{:?} {:?}", &slice.as_mut()[..20], &control[..20]);
            std::process::exit(9)
        }
        keep_slice_test_p(slice, control, ms);
    }
}

fn criterion_benchmark(c: &mut Criterion) {
    let input = DATA;

    let mut c = c.benchmark_group("sample-size-example");

    c.sample_size(500);
    //c.bench_function("with pool threaded test", |b| {
    //    b.iter(|| with_pool_trreaded_test_1(&input[..]))
    //});
    //c.bench_function("with pool threaded test 2", |b| {
    //    b.iter(|| with_pool_trreaded_test_2(&input[..]))
    //});
    c.bench_function("with pool", |b| b.iter(|| with_pool(&input[..])));
    c.bench_function("without pool", |b| b.iter(|| without_pool(&input[..])));
    c.bench_function("with control struct", |b| {
        b.iter(|| with_contro_struct(&input[..]))
    });
    c.bench_function("with control struct max e", |b| {
        b.iter(|| with_contro_struct_max_e(&input[..]))
    });
    c.bench_function("with pool thread 1", |b| {
        b.iter(|| with_pool_trreaded_1(&input[..]))
    });
    c.bench_function("without pool threaded 1", |b| {
        b.iter(|| without_pool_threaded_1(&input[..]))
    });
    c.bench_function("with control threaded 1", |b| {
        b.iter(|| with_control_threaded(&input[..]))
    });
    c.bench_function("with control threaded max", |b| {
        b.iter(|| with_control_max_threaded(&input[..]))
    });
    c.bench_function("with pool thread 2", |b| {
        b.iter(|| with_pool_trreaded_2(&input[..]))
    });
    c.bench_function("without pool threaded 2", |b| {
        b.iter(|| without_pool_threaded_2(&input[..]))
    });
    c.bench_function("with control threaded 2", |b| {
        b.iter(|| with_control_threaded_2(&input[..]))
    });
    c.bench_function("with control threaded max 2", |b| {
        b.iter(|| with_control_max_threaded_2(&input[..]))
    });
    c.finish();
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);

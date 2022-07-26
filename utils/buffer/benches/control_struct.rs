use buffer_sv2::{Buffer, Slice};
use std::sync::{Arc, Mutex};

use core::{sync::atomic::Ordering, time::Duration};
use rand::Rng;

const FILE_LEN: usize = 5242880;
pub const DATA: &[u8; FILE_LEN] = include_bytes!("../fuzz/random");

#[inline(always)]
pub fn add_random_bytes(message_len: usize, buffer: &mut impl Buffer, input: &[u8]) {
    let rounds = message_len / 10;

    for i in 0..rounds {
        let writable: &mut [u8] = buffer.get_writable(10).as_mut();
        writable.copy_from_slice(&input[i..i + 10]);
    }
}

pub trait Load: AsMut<[u8]> {
    fn load(&mut self) -> usize;
}

impl Load for Vec<u8> {
    #[inline(always)]
    fn load(&mut self) -> usize {
        self.len()
    }
}

impl Load for Slice {
    #[inline(always)]
    fn load(&mut self) -> usize {
        self.shared_state.load(Ordering::SeqCst) as usize
    }
}

#[inline(always)]
pub fn keep_slice(mut slice: impl Load + Send + 'static, d: Duration) {
    std::thread::spawn(move || {
        let _time = (2_usize.pow(16) - slice.as_mut().len()) / 1000;
        std::thread::sleep(d);
        let _ = slice.load();
    });
}

#[inline(always)]
pub fn bench_no_thread(mut pool: impl Buffer, data: &[u8]) {
    let mut rng = rand::thread_rng();
    for i in 0..1000 {
        let message_length = 2_usize.pow(14) - rng.gen_range(0..12000);
        add_random_bytes(message_length, &mut pool, &data[i..]);

        pool.get_data_owned();

        add_random_bytes(message_length, &mut pool, &data[i..]);

        pool.get_data_owned();
    }
}

impl Load for SSlice {
    #[inline(always)]
    fn load(&mut self) -> usize {
        78
    }
}

impl Load for MaxESlice {
    #[inline(always)]
    fn load(&mut self) -> usize {
        78
    }
}

use std::collections::BTreeMap;
pub struct PPool {
    pool: BTreeMap<u8, Vec<u8>>,
    free_slots: Vec<u8>,
    to_free: Arc<Mutex<Vec<u8>>>,
}

pub struct SSlice {
    offset: *mut u8,
    len: usize,
    index: u8,
    to_free: Arc<Mutex<Vec<u8>>>,
}

impl From<SSlice> for Slice {
    fn from(_v: SSlice) -> Self {
        todo!()
    }
}

impl AsMut<[u8]> for SSlice {
    #[inline(always)]
    fn as_mut(&mut self) -> &mut [u8] {
        unsafe { core::slice::from_raw_parts_mut(self.offset, self.len) }
    }
}

impl AsRef<[u8]> for SSlice {
    #[inline(always)]
    fn as_ref(&self) -> &[u8] {
        unsafe { core::slice::from_raw_parts(self.offset, self.len) }
    }
}

impl Drop for SSlice {
    fn drop(&mut self) {
        let mut to_free = self.to_free.lock().unwrap();
        to_free.push(self.index);
    }
}

impl PPool {
    pub fn new(capacity: usize) -> Self {
        let mut free_slots: Vec<u8> = Vec::with_capacity(255);
        let mut map: BTreeMap<u8, Vec<u8>> = BTreeMap::new();
        for k in 0..255 {
            let mut b = vec![0; capacity];
            unsafe { b.set_len(0) };
            map.insert(k, b);
            free_slots.push(k);
        }
        let to_free = Arc::new(Mutex::new(Vec::new()));
        Self {
            pool: map,
            free_slots,
            to_free,
        }
    }

    #[inline(never)]
    pub fn free(&mut self) {
        let mut slots = self.to_free.lock().unwrap();
        for _ in 0..slots.len() {
            let slot = slots.pop().unwrap();
            self.free_slots.push(slot);
            let b = self.pool.get_mut(&slot).unwrap();
            unsafe { b.set_len(0) };
        }
    }
}

impl Buffer for PPool {
    type Slice = SSlice;

    #[inline(always)]
    fn get_writable(&mut self, len: usize) -> &mut [u8] {
        if self.free_slots.len() > 0 {
            let slot = self.free_slots[self.free_slots.len() - 1];

            let b = self.pool.get_mut(&slot).unwrap();
            let offset = b.len();

            if offset + len <= b.capacity() {
                unsafe { b.set_len(offset + len) };
                &mut b[offset..offset + len]
            } else {
                panic!()
            }
        } else {
            self.free();
            self.get_writable(len)
        }
    }

    #[inline(always)]
    fn get_data_owned(&mut self) -> Self::Slice {
        let slot = self.free_slots.pop().unwrap();

        let b = self.pool.get_mut(&slot).unwrap();

        let offset = b.as_mut_ptr();

        SSlice {
            offset,
            len: b.len(),
            index: slot,
            to_free: self.to_free.clone(),
        }
    }

    fn get_data_by_ref(&mut self, _len: usize) -> &mut [u8] {
        todo!()
    }

    fn len(&self) -> usize {
        todo!()
    }
}

unsafe impl Send for SSlice {}

pub struct MaxEfficeincy {
    inner: Vec<u8>,
}

impl MaxEfficeincy {
    pub fn new(capacity: usize) -> Self {
        let inner = vec![0; capacity];
        Self { inner }
    }
}

pub struct MaxESlice {
    offset: *mut u8,
    len: usize,
}

impl From<MaxESlice> for Slice {
    fn from(_v: MaxESlice) -> Self {
        todo!()
    }
}

impl AsMut<[u8]> for MaxESlice {
    #[inline(always)]
    fn as_mut(&mut self) -> &mut [u8] {
        unsafe { core::slice::from_raw_parts_mut(self.offset, self.len) }
    }
}

impl AsRef<[u8]> for MaxESlice {
    #[inline(always)]
    fn as_ref(&self) -> &[u8] {
        unsafe { core::slice::from_raw_parts(self.offset, self.len) }
    }
}

unsafe impl Send for MaxESlice {}

impl Buffer for MaxEfficeincy {
    type Slice = MaxESlice;

    #[inline(always)]
    fn get_writable(&mut self, len: usize) -> &mut [u8] {
        &mut self.inner[0..len]
    }

    #[inline(always)]
    fn get_data_owned(&mut self) -> Self::Slice {
        let offset = self.inner.as_mut_ptr();

        MaxESlice {
            offset,
            len: self.inner.len(),
        }
    }

    fn get_data_by_ref(&mut self, _len: usize) -> &mut [u8] {
        todo!()
    }

    fn len(&self) -> usize {
        todo!()
    }
}

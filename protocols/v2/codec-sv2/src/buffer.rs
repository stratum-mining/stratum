pub trait Buffer {
    type Slice: AsMut<[u8]>;

    fn get_writable(&mut self, len: usize) -> &mut [u8];

    fn get_data_owned(&mut self) -> Self::Slice;

    fn get_data_by_ref(&mut self, header_size: usize) -> &mut [u8];

    fn len(&self) -> usize;
}

pub struct SlowAndCorrect {
    inner: Vec<u8>,
    cursor: usize,
}

impl SlowAndCorrect {
    pub fn new() -> Self {
        Self {
            inner: Vec::new(),
            cursor: 0,
        }
    }
}

impl Default for SlowAndCorrect {
    fn default() -> Self {
        Self::new()
    }
}

impl Buffer for SlowAndCorrect {
    type Slice = Vec<u8>;

    #[inline]
    fn get_writable(&mut self, len: usize) -> &mut [u8] {
        let cursor = self.cursor;
        let len = self.cursor + len;

        if len > self.inner.len() {
            self.inner.resize(len, 0)
        };

        self.cursor = len;

        &mut self.inner[cursor..len]
    }

    #[inline]
    fn get_data_owned(&mut self) -> Vec<u8> {
        let mut tail = self.inner.split_off(self.cursor);
        core::mem::swap(&mut tail, &mut self.inner);
        let head = tail;
        self.cursor = 0;
        head
    }

    #[inline]
    fn get_data_by_ref(&mut self, header_size: usize) -> &mut [u8] {
        &mut self.inner[..usize::min(header_size, self.cursor)]
    }

    #[inline]
    fn len(&self) -> usize {
        self.cursor
    }
}

// Kind of preallocated ring buffer to decode incoming messages.
// Should be faster than the other one. It resuse the space when the frame are dropped. If a frame
// have a short life and first created frames are dropped first then latter created frame, it
// should be faster that the orther type or then a mempool that use a map, btw these are only
// assumptions.
//
// It can be:
// void -> no meningfull data is in the buffer
// with raw data -> it contain a part of an incoming message
// with [frame] -> it contain a complete message/s
// with [frame] and raw -> it contain a complete message/s and some other data that can be a part of a
//     message or a non parsed complete message
//
// Read incoming
// inner = [,.,.,.,.,.,.,.,.,.,.,.,.,.,.,.]
//         |     |
//           raw
//
// Read until a valid frame is in the buffer
// inner = [,.,.,.,.,.,.,.,.,.,.,.,.,.,.,.]
//         |           |
//             frame
//
// Start parsing the next frame
// inner = [,.,.,.,.,.,.,.,.,.,.,.,.,.,.,.]
//         |           |   |
//             frame    raw
//
// Free the first frame
// inner = [,.,.,.,.,.,.,.,.,.,.,.,.,.,.,.]
//                     |   |
//                      raw
//
// Finish to parse the next frame
// inner = [,.,.,.,.,.,.,.,.,.,.,.,.,.,.,.]
//                     |         |
//                        frame
//
// Some bytes of the next frame have be parsed but there is no space to parse the reamaining bytes
// at the end of the buffer. This shouldn't happen very often as the buffer always start to write
// raw data in the part that has more free space
// inner = [,.,.,.,.,.,.,.,.,.,.,.,.,.,.,.]
//                                  |   |
//                                   raw
//
// The  parsed bytes are copied at the begenning of the buffer that becasue is easier to work with
// memory in contigous space
// inner = [,.,.,.,.,.,.,.,.,.,.,.,.,.,.,.]
//          |   |
//           raw
//
// If there is no enaugh space at the end and at the begennning of the buffer increase the buffer
// size
//
// struct ProbablyFaster_ {
//     inner: BytesMut,
//     last_b: usize,
//     to_free: std::collections::BinaryHeap<ComparableBytes>,
// }
//
//
// #[derive(Eq)]
// struct ComparableBytes ((usize, BytesMut));
//
// impl Ord for ComparableBytes {
//     fn cmp(&self, other: &Self) -> std::cmp::Ordering {
//         other.0.0.cmp(&self.0.0)
//     }
// }
//
// impl PartialEq for ComparableBytes {
//     fn eq(&self, other: &Self) -> bool {
//         self.0.0 == other.0.0
//     }
// }
//
//
// impl PartialOrd for ComparableBytes {
//     fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
//         Some(self.cmp(other))
//     }
// }
//
// #[derive(Clone)]
// pub struct ProbablyFaster(Arc<Mutex<ProbablyFaster_>>);
//
//
// impl ProbablyFaster {
//
//     fn get_slice(&self, len: usize) -> Slice {
//         let mut self_ = self.0.lock().unwrap();
//
//         let mut tail = self_.inner.split_off(len);
//         core::mem::swap(&mut tail, &mut self_.inner);
//         let slice = tail;
//         self_.last_b += 1;
//
//         Slice { slice, id: self_.last_b, buffer: self.clone() }
//     }
//
//     #[inline]
//     fn free(&mut self, id: usize, b: &mut BytesMut) {
//         let mut self_ = self.0.lock().unwrap();
//
//         // this do not allocate as it call Vec::with_capacity that do not allocate for 0
//         let mut to_drop = BytesMut::with_capacity(0);
//         core::mem::swap(&mut to_drop, b);
//
//         if self_.last_b == id {
//             self_.last_b -= 1;
//             self_.inner.unsplit(to_drop);
//             if self_.to_free.len() > 0 {
//                 Self::free_next(&mut self_);
//             }
//         } else {
//             Self::add_to_free(&mut self_, id, to_drop);
//         }
//     }
//
//     #[inline(never)]
//     fn free_next(inner: &mut ProbablyFaster_) {
//         let (next_id, next) = inner.to_free.pop().unwrap().0;
//         if inner.last_b == next_id {
//             inner.last_b -= 1;
//             inner.inner.unsplit(next);
//             if inner.to_free.len() > 0 {
//                 Self::free_next(inner);
//             }
//         };
//     }
//
//     #[inline(never)]
//     fn add_to_free(inner_: &mut ProbablyFaster_, id: usize, b: BytesMut) {
//         inner_.to_free.push(ComparableBytes((id, b)))
//     }
//
// }
//
// pub struct Slice {
//     buffer: ProbablyFaster,
//     slice: BytesMut,
//     id: usize,
// }
//
// impl Slice {
//
//     #[inline]
//     fn get_mut(&mut self) -> &mut [u8] {
//         &mut self.slice[..]
//     }
// }
//
// impl Drop for Slice {
//     fn drop(&mut self) {
//         self.buffer.free(self.id, &mut self.slice);
//     }
// }

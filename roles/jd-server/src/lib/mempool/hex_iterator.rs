use std::{io, io::Error};
/// Iterator over a hex-encoded string slice which decodes hex and yields bytes.
pub struct HexIterator<'a> {
    /// The `Bytes` iterator whose next two bytes will be decoded to yield
    /// the next byte.
    iter: std::str::Bytes<'a>,
}

impl<'a> HexIterator<'a> {
    /// Constructs a new `HexIterator` from a string slice.
    ///
    /// # Errors
    ///
    /// If the input string is of odd length.
    pub fn new(s: &'a str) -> Result<HexIterator<'a>, Error> {
        if s.len() % 2 != 0 {
            panic!("the length must be even!");
            //Err(Error::OddLengthString(s.len()))
        } else {
            Ok(HexIterator { iter: s.bytes() })
        }
    }
}
impl<'a> Iterator for HexIterator<'a> {
    type Item = Result<u8, HexError>;

    fn next(&mut self) -> Option<Result<u8, HexError>> {
        let hi = self.iter.next()?;
        let lo = self.iter.next().unwrap();
        Some(chars_to_hex(hi, lo))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let (min, max) = self.iter.size_hint();
        (min / 2, max.map(|x| x / 2))
    }
}

impl<'a> io::Read for HexIterator<'a> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let mut bytes_read = 0usize;
        for dst in buf {
            match self.next() {
                Some(Ok(src)) => {
                    *dst = src;
                    bytes_read += 1;
                }
                _ => break,
            }
        }
        Ok(bytes_read)
    }
}

fn chars_to_hex(hi: u8, lo: u8) -> Result<u8, HexError> {
    let hih = (hi as char).to_digit(16).ok_or(HexError::InvalidChar(hi))?;
    let loh = (lo as char).to_digit(16).ok_or(HexError::InvalidChar(lo))?;

    let ret = (hih << 4) + loh;
    Ok(ret as u8)
}

pub enum HexError {
    /// Non-hexadecimal character.
    InvalidChar(u8),
    // Purported hex string had odd length.
    //OddLengthString(usize),
    // Tried to parse fixed-length hash from a string with the wrong type (expected, got).
    //InvalidLength(usize, usize),
}

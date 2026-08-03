// Provides separate borrowed and owned byte wrappers for SV2 byte-array primitives.
use crate::{
    codec::{GetSize, SizeHint},
    datatypes::Sv2DataType,
    Error,
};

use alloc::{string::String, vec::Vec};
use core::{
    convert::{TryFrom, TryInto},
    fmt,
};

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct Inner<
    'a,
    const ISFIXED: bool,
    const SIZE: usize,
    const HEADERSIZE: usize,
    const MAXSIZE: usize,
> {
    data: &'a [u8],
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct InnerOwned<
    const ISFIXED: bool,
    const SIZE: usize,
    const HEADERSIZE: usize,
    const MAXSIZE: usize,
> {
    data: Vec<u8>,
}

trait InnerBytes {
    const ISFIXED: bool;
    const SIZE: usize;
    const HEADERSIZE: usize;
    const MAXSIZE: usize;

    fn as_bytes(&self) -> &[u8];

    fn len(&self) -> usize {
        let len = self.as_bytes().len();
        if Self::ISFIXED {
            debug_assert_eq!(len, Self::SIZE);
            Self::SIZE
        } else {
            len
        }
    }

    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn hex_string(&self) -> String {
        let mut hex = String::with_capacity(self.as_bytes().len() * 2);
        write_hex(self.as_bytes().iter(), &mut hex).expect("writing to String cannot fail");
        hex
    }

    fn reversed_hex_string(&self) -> String {
        let mut hex = String::with_capacity(self.as_bytes().len() * 2);
        write_hex(self.as_bytes().iter().rev(), &mut hex).expect("writing to String cannot fail");
        hex
    }

    fn try_as_array<const N: usize>(&self) -> Result<[u8; N], Error> {
        self.as_bytes()
            .try_into()
            .map_err(|_| Error::ReadError(self.as_bytes().len(), N))
    }
}

impl<'a, const ISFIXED: bool, const SIZE: usize, const HEADERSIZE: usize, const MAXSIZE: usize>
    InnerBytes for Inner<'a, ISFIXED, SIZE, HEADERSIZE, MAXSIZE>
{
    const ISFIXED: bool = ISFIXED;
    const SIZE: usize = SIZE;
    const HEADERSIZE: usize = HEADERSIZE;
    const MAXSIZE: usize = MAXSIZE;

    fn as_bytes(&self) -> &[u8] {
        self.data
    }
}

impl<const ISFIXED: bool, const SIZE: usize, const HEADERSIZE: usize, const MAXSIZE: usize>
    InnerBytes for InnerOwned<ISFIXED, SIZE, HEADERSIZE, MAXSIZE>
{
    const ISFIXED: bool = ISFIXED;
    const SIZE: usize = SIZE;
    const HEADERSIZE: usize = HEADERSIZE;
    const MAXSIZE: usize = MAXSIZE;

    fn as_bytes(&self) -> &[u8] {
        &self.data
    }
}

impl<'a, const ISFIXED: bool, const SIZE: usize, const HEADERSIZE: usize, const MAXSIZE: usize>
    PartialEq<InnerOwned<ISFIXED, SIZE, HEADERSIZE, MAXSIZE>>
    for Inner<'a, ISFIXED, SIZE, HEADERSIZE, MAXSIZE>
{
    fn eq(&self, other: &InnerOwned<ISFIXED, SIZE, HEADERSIZE, MAXSIZE>) -> bool {
        self.as_bytes() == other.as_bytes()
    }
}

impl<'a, const ISFIXED: bool, const SIZE: usize, const HEADERSIZE: usize, const MAXSIZE: usize>
    PartialEq<Inner<'a, ISFIXED, SIZE, HEADERSIZE, MAXSIZE>>
    for InnerOwned<ISFIXED, SIZE, HEADERSIZE, MAXSIZE>
{
    fn eq(&self, other: &Inner<'a, ISFIXED, SIZE, HEADERSIZE, MAXSIZE>) -> bool {
        self.as_bytes() == other.as_bytes()
    }
}

fn write_hex<'a>(
    data: impl IntoIterator<Item = &'a u8>,
    writer: &mut impl fmt::Write,
) -> fmt::Result {
    for byte in data {
        write!(writer, "{byte:02x}")?;
    }
    Ok(())
}

fn max_encodable_len(header_size: usize) -> usize {
    match header_size {
        1 => u8::MAX as usize,
        2 => u16::MAX as usize,
        3 => 0x00ff_ffff,
        _ => unreachable!("variable SV2 byte wrapper HEADERSIZE was validated"),
    }
}

fn validate_configuration(is_fixed: bool, header_size: usize, max_size: usize) {
    if is_fixed {
        assert_eq!(
            header_size, 0,
            "fixed SV2 byte wrapper HEADERSIZE must be 0"
        );
    } else {
        assert!(
            (1..=3).contains(&header_size),
            "variable SV2 byte wrapper HEADERSIZE must be in 1..=3"
        );
        assert!(
            max_size <= max_encodable_len(header_size),
            "variable SV2 byte wrapper MAXSIZE must fit in HEADERSIZE"
        );
    }
}

fn write_inner_to_slice<T: InnerBytes>(value: &T, dst: &mut [u8]) -> Result<usize, Error> {
    let header_size = T::HEADERSIZE;
    validate_configuration(T::ISFIXED, T::HEADERSIZE, T::MAXSIZE);

    let payload_len = value.len();
    let size = payload_len + header_size;
    if dst.len() < size {
        return Err(Error::WriteError(size, dst.len()));
    }

    let len_bytes = payload_len.to_le_bytes();
    dst[..header_size].copy_from_slice(&len_bytes[..header_size]);
    dst[header_size..size].copy_from_slice(value.as_bytes());
    Ok(size)
}

fn from_bytes_inner<
    'a,
    T: TryFrom<&'a [u8], Error = Error>,
    const ISFIXED: bool,
    const SIZE: usize,
    const HEADERSIZE: usize,
    const MAXSIZE: usize,
>(
    data: &'a mut [u8],
) -> Result<T, Error> {
    let size = size_hint_inner::<ISFIXED, SIZE, HEADERSIZE, MAXSIZE>(data, 0)?;
    if size > data.len() {
        return Err(Error::ReadError(data.len(), size));
    }
    let payload = if ISFIXED {
        &data[..size]
    } else {
        &data[HEADERSIZE..size]
    };
    payload.try_into()
}

fn size_hint_inner<
    const ISFIXED: bool,
    const SIZE: usize,
    const HEADERSIZE: usize,
    const MAXSIZE: usize,
>(
    data: &[u8],
    offset: usize,
) -> Result<usize, Error> {
    if offset > data.len() {
        return Err(Error::ReadError(data.len(), offset));
    }
    expected_length::<ISFIXED, SIZE, HEADERSIZE, MAXSIZE>(&data[offset..])
}

fn expected_length<
    const ISFIXED: bool,
    const SIZE: usize,
    const HEADERSIZE: usize,
    const MAXSIZE: usize,
>(
    data: &[u8],
) -> Result<usize, Error> {
    validate_configuration(ISFIXED, HEADERSIZE, MAXSIZE);

    if ISFIXED {
        Ok(SIZE)
    } else {
        expected_length_variable::<ISFIXED, SIZE, HEADERSIZE, MAXSIZE>(data)
    }
}

fn expected_length_variable<
    const ISFIXED: bool,
    const SIZE: usize,
    const HEADERSIZE: usize,
    const MAXSIZE: usize,
>(
    data: &[u8],
) -> Result<usize, Error> {
    if data.len() < HEADERSIZE {
        return Err(Error::ReadError(data.len(), HEADERSIZE));
    }

    let payload_len = match HEADERSIZE {
        1 => data[0] as usize,
        2 => u16::from_le_bytes([data[0], data[1]]) as usize,
        3 => u32::from_le_bytes([data[0], data[1], data[2], 0]) as usize,
        _ => unreachable!("variable SV2 byte wrapper HEADERSIZE was validated"),
    };

    if payload_len <= MAXSIZE {
        Ok(payload_len + HEADERSIZE)
    } else {
        Err(Error::ValueExceedsMaxSize(
            ISFIXED,
            SIZE,
            HEADERSIZE,
            MAXSIZE,
            data.to_vec(),
            payload_len,
        ))
    }
}

fn validate_payload<
    const ISFIXED: bool,
    const SIZE: usize,
    const HEADERSIZE: usize,
    const MAXSIZE: usize,
>(
    value: &[u8],
) -> Result<(), Error> {
    validate_configuration(ISFIXED, HEADERSIZE, MAXSIZE);

    if ISFIXED {
        return if value.len() == SIZE {
            Ok(())
        } else {
            Err(Error::ValueExceedsMaxSize(
                ISFIXED,
                SIZE,
                HEADERSIZE,
                MAXSIZE,
                value.to_vec(),
                value.len(),
            ))
        };
    }

    if value.len() > MAXSIZE {
        Err(Error::ValueExceedsMaxSize(
            ISFIXED,
            SIZE,
            HEADERSIZE,
            MAXSIZE,
            value.to_vec(),
            value.len(),
        ))
    } else {
        Ok(())
    }
}

impl<'a, const ISFIXED: bool, const SIZE: usize, const HEADERSIZE: usize, const MAXSIZE: usize>
    Inner<'a, ISFIXED, SIZE, HEADERSIZE, MAXSIZE>
{
    pub fn new(data: &'a [u8]) -> Result<Self, Error> {
        validate_payload::<ISFIXED, SIZE, HEADERSIZE, MAXSIZE>(data)?;
        Ok(Self { data })
    }

    pub fn len(&self) -> usize {
        InnerBytes::len(self)
    }

    pub fn is_empty(&self) -> bool {
        InnerBytes::is_empty(self)
    }

    pub fn as_bytes(&self) -> &[u8] {
        InnerBytes::as_bytes(self)
    }

    pub fn hex_string(&self) -> String {
        InnerBytes::hex_string(self)
    }

    pub fn reversed_hex_string(&self) -> String {
        InnerBytes::reversed_hex_string(self)
    }

    pub fn to_owned_bytes(self) -> Vec<u8> {
        self.data.to_vec()
    }

    pub fn try_as_array<const N: usize>(&self) -> Result<[u8; N], Error> {
        InnerBytes::try_as_array(self)
    }

    pub fn into_owned(self) -> InnerOwned<ISFIXED, SIZE, HEADERSIZE, MAXSIZE> {
        InnerOwned {
            data: self.data.to_vec(),
        }
    }
}

impl<const SIZE: usize> Inner<'_, true, SIZE, 0, 0> {
    pub fn as_array(&self) -> &[u8; SIZE] {
        self.as_bytes()
            .try_into()
            .expect("fixed-size SV2 byte wrapper must always match SIZE")
    }

    pub fn to_array(self) -> [u8; SIZE] {
        *self.as_array()
    }

    pub fn into_array(self) -> [u8; SIZE] {
        self.as_bytes()
            .try_into()
            .expect("fixed-size SV2 byte wrapper must always match SIZE")
    }
}

impl<const ISFIXED: bool, const SIZE: usize, const HEADERSIZE: usize, const MAXSIZE: usize>
    InnerOwned<ISFIXED, SIZE, HEADERSIZE, MAXSIZE>
{
    pub fn new(data: Vec<u8>) -> Result<Self, Error> {
        validate_payload::<ISFIXED, SIZE, HEADERSIZE, MAXSIZE>(&data)?;
        Ok(Self { data })
    }

    pub fn len(&self) -> usize {
        InnerBytes::len(self)
    }

    pub fn is_empty(&self) -> bool {
        InnerBytes::is_empty(self)
    }

    pub fn as_bytes(&self) -> &[u8] {
        InnerBytes::as_bytes(self)
    }

    pub fn hex_string(&self) -> String {
        InnerBytes::hex_string(self)
    }

    pub fn reversed_hex_string(&self) -> String {
        InnerBytes::reversed_hex_string(self)
    }

    pub fn as_mut_bytes(&mut self) -> &mut [u8] {
        &mut self.data
    }

    pub fn to_owned_bytes(&self) -> Vec<u8> {
        self.data.clone()
    }

    pub fn into_bytes(self) -> Vec<u8> {
        self.data
    }

    pub fn try_as_array<const N: usize>(&self) -> Result<[u8; N], Error> {
        InnerBytes::try_as_array(self)
    }
}

impl<const SIZE: usize> InnerOwned<true, SIZE, 0, 0> {
    pub fn as_array(&self) -> &[u8; SIZE] {
        self.as_bytes()
            .try_into()
            .expect("fixed-size SV2 byte wrapper must always match SIZE")
    }

    pub fn to_array(&self) -> [u8; SIZE] {
        *self.as_array()
    }

    pub fn into_array(self) -> [u8; SIZE] {
        self.data
            .try_into()
            .expect("fixed-size SV2 byte wrapper must always match SIZE")
    }
}

impl<'a, const ISFIXED: bool, const SIZE: usize, const HEADERSIZE: usize, const MAXSIZE: usize>
    TryFrom<&'a [u8]> for Inner<'a, ISFIXED, SIZE, HEADERSIZE, MAXSIZE>
{
    type Error = Error;

    fn try_from(value: &'a [u8]) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl<'a, const ISFIXED: bool, const SIZE: usize, const HEADERSIZE: usize, const MAXSIZE: usize>
    TryFrom<&'a mut [u8]> for Inner<'a, ISFIXED, SIZE, HEADERSIZE, MAXSIZE>
{
    type Error = Error;

    fn try_from(value: &'a mut [u8]) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl<'a, const SIZE: usize> From<&'a [u8; SIZE]> for Inner<'a, true, SIZE, 0, 0> {
    fn from(value: &'a [u8; SIZE]) -> Self {
        Self { data: &value[..] }
    }
}

impl<'a, const SIZE: usize> From<&'a mut [u8; SIZE]> for Inner<'a, true, SIZE, 0, 0> {
    fn from(value: &'a mut [u8; SIZE]) -> Self {
        Self { data: &value[..] }
    }
}

impl<'a, const N: usize, const SIZE: usize, const HEADERSIZE: usize, const MAXSIZE: usize>
    TryFrom<&'a [u8; N]> for Inner<'a, false, SIZE, HEADERSIZE, MAXSIZE>
{
    type Error = Error;

    fn try_from(value: &'a [u8; N]) -> Result<Self, Self::Error> {
        Self::new(&value[..])
    }
}

impl<'a, const N: usize, const SIZE: usize, const HEADERSIZE: usize, const MAXSIZE: usize>
    TryFrom<&'a mut [u8; N]> for Inner<'a, false, SIZE, HEADERSIZE, MAXSIZE>
{
    type Error = Error;

    fn try_from(value: &'a mut [u8; N]) -> Result<Self, Self::Error> {
        Self::new(&value[..])
    }
}

impl<const ISFIXED: bool, const SIZE: usize, const HEADERSIZE: usize, const MAXSIZE: usize>
    TryFrom<Vec<u8>> for InnerOwned<ISFIXED, SIZE, HEADERSIZE, MAXSIZE>
{
    type Error = Error;

    fn try_from(value: Vec<u8>) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl<const ISFIXED: bool, const SIZE: usize, const HEADERSIZE: usize, const MAXSIZE: usize>
    TryFrom<&[u8]> for InnerOwned<ISFIXED, SIZE, HEADERSIZE, MAXSIZE>
{
    type Error = Error;

    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        Self::new(value.to_vec())
    }
}

impl<const SIZE: usize> From<[u8; SIZE]> for InnerOwned<true, SIZE, 0, 0> {
    fn from(value: [u8; SIZE]) -> Self {
        Self { data: value.into() }
    }
}

impl<const SIZE: usize> From<&[u8; SIZE]> for InnerOwned<true, SIZE, 0, 0> {
    fn from(value: &[u8; SIZE]) -> Self {
        Self {
            data: value.to_vec(),
        }
    }
}

impl<const SIZE: usize> From<&mut [u8; SIZE]> for InnerOwned<true, SIZE, 0, 0> {
    fn from(value: &mut [u8; SIZE]) -> Self {
        Self {
            data: value.to_vec(),
        }
    }
}

impl<const N: usize, const SIZE: usize, const HEADERSIZE: usize, const MAXSIZE: usize>
    TryFrom<[u8; N]> for InnerOwned<false, SIZE, HEADERSIZE, MAXSIZE>
{
    type Error = Error;

    fn try_from(value: [u8; N]) -> Result<Self, Self::Error> {
        Self::new(value.to_vec())
    }
}

impl<const N: usize, const SIZE: usize, const HEADERSIZE: usize, const MAXSIZE: usize>
    TryFrom<&[u8; N]> for InnerOwned<false, SIZE, HEADERSIZE, MAXSIZE>
{
    type Error = Error;

    fn try_from(value: &[u8; N]) -> Result<Self, Self::Error> {
        Self::new(value.to_vec())
    }
}

impl<const N: usize, const SIZE: usize, const HEADERSIZE: usize, const MAXSIZE: usize>
    TryFrom<&mut [u8; N]> for InnerOwned<false, SIZE, HEADERSIZE, MAXSIZE>
{
    type Error = Error;

    fn try_from(value: &mut [u8; N]) -> Result<Self, Self::Error> {
        Self::new(value.to_vec())
    }
}

impl<const ISFIXED: bool, const SIZE: usize, const HEADERSIZE: usize, const MAXSIZE: usize> GetSize
    for Inner<'_, ISFIXED, SIZE, HEADERSIZE, MAXSIZE>
{
    fn get_size(&self) -> usize {
        self.len() + HEADERSIZE
    }
}

impl<const ISFIXED: bool, const SIZE: usize, const HEADERSIZE: usize, const MAXSIZE: usize> GetSize
    for InnerOwned<ISFIXED, SIZE, HEADERSIZE, MAXSIZE>
{
    fn get_size(&self) -> usize {
        self.len() + HEADERSIZE
    }
}

impl<const ISFIXED: bool, const SIZE: usize, const HEADERSIZE: usize, const MAXSIZE: usize> SizeHint
    for Inner<'_, ISFIXED, SIZE, HEADERSIZE, MAXSIZE>
{
    fn size_hint(data: &[u8], offset: usize) -> Result<usize, Error> {
        size_hint_inner::<ISFIXED, SIZE, HEADERSIZE, MAXSIZE>(data, offset)
    }

    fn size_hint_(&self, data: &[u8], offset: usize) -> Result<usize, Error> {
        Self::size_hint(data, offset)
    }
}

impl<const ISFIXED: bool, const SIZE: usize, const HEADERSIZE: usize, const MAXSIZE: usize> SizeHint
    for InnerOwned<ISFIXED, SIZE, HEADERSIZE, MAXSIZE>
{
    fn size_hint(data: &[u8], offset: usize) -> Result<usize, Error> {
        size_hint_inner::<ISFIXED, SIZE, HEADERSIZE, MAXSIZE>(data, offset)
    }

    fn size_hint_(&self, data: &[u8], offset: usize) -> Result<usize, Error> {
        Self::size_hint(data, offset)
    }
}

use crate::codec::decodable::FieldMarker;

impl<'a, const ISFIXED: bool, const SIZE: usize, const HEADERSIZE: usize, const MAXSIZE: usize>
    Sv2DataType<'a> for Inner<'a, ISFIXED, SIZE, HEADERSIZE, MAXSIZE>
where
    Self: TryInto<FieldMarker>,
{
    fn from_bytes_(data: &'a mut [u8]) -> Result<Self, Error> {
        from_bytes_inner::<Self, ISFIXED, SIZE, HEADERSIZE, MAXSIZE>(data)
    }

    fn to_slice(&'a self, dst: &mut [u8]) -> Result<usize, Error> {
        write_inner_to_slice(self, dst)
    }
}

impl<'a, const ISFIXED: bool, const SIZE: usize, const HEADERSIZE: usize, const MAXSIZE: usize>
    Sv2DataType<'a> for InnerOwned<ISFIXED, SIZE, HEADERSIZE, MAXSIZE>
where
    Self: TryInto<FieldMarker>,
{
    fn from_bytes_(data: &'a mut [u8]) -> Result<Self, Error> {
        from_bytes_inner::<Self, ISFIXED, SIZE, HEADERSIZE, MAXSIZE>(data)
    }

    fn to_slice(&'a self, dst: &mut [u8]) -> Result<usize, Error> {
        write_inner_to_slice(self, dst)
    }
}

impl<const ISFIXED: bool, const SIZE: usize, const HEADERSIZE: usize, const MAXSIZE: usize>
    AsRef<[u8]> for Inner<'_, ISFIXED, SIZE, HEADERSIZE, MAXSIZE>
{
    fn as_ref(&self) -> &[u8] {
        self.as_bytes()
    }
}

impl<const ISFIXED: bool, const SIZE: usize, const HEADERSIZE: usize, const MAXSIZE: usize>
    AsRef<[u8]> for InnerOwned<ISFIXED, SIZE, HEADERSIZE, MAXSIZE>
{
    fn as_ref(&self) -> &[u8] {
        self.as_bytes()
    }
}

#[cfg(test)]
mod test {
    use super::{Inner, InnerOwned};
    use crate::{B032Owned, Error, GetSize, SignatureOwned, SizeHint, U256Owned, B032, U256};
    extern crate std;
    use self::std::panic::catch_unwind;

    #[test]
    fn fixed_inner_len_reports_real_size() {
        let u256: U256Owned = vec![0u8; 32].try_into().unwrap();
        assert_eq!(u256.len(), 32, "U256::len() must be 32, not 1");
        assert_eq!(u256.len(), u256.get_size());

        let sig: SignatureOwned = vec![0u8; 64].try_into().unwrap();
        assert_eq!(sig.len(), 64, "Signature::len() must be 64, not 1");
    }

    #[test]
    fn borrowed_fixed_inner_from_array_refs() {
        let bytes = [7u8; 32];
        let u256 = U256::from(&bytes);
        assert_eq!(u256.as_bytes(), &bytes);

        let mut mutable_bytes = [8u8; 32];
        let u256 = U256::from(&mut mutable_bytes);
        assert_eq!(u256.as_bytes(), &[8u8; 32]);
    }

    #[test]
    fn borrowed_variable_inner_try_from_array_refs() {
        let bytes = [9u8; 32];
        let b032 = B032::try_from(&bytes).unwrap();
        assert_eq!(b032.as_bytes(), &bytes);

        let mut mutable_bytes = [10u8; 32];
        let b032 = B032::try_from(&mut mutable_bytes).unwrap();
        assert_eq!(b032.as_bytes(), &[10u8; 32]);

        let oversized = [0u8; 33];
        assert!(B032::try_from(&oversized).is_err());
    }

    #[test]
    fn owned_inner_from_arrays_and_array_refs() {
        let fixed = [11u8; 32];
        assert_eq!(U256Owned::from(fixed).as_bytes(), &[11u8; 32]);

        let fixed_ref = [12u8; 32];
        assert_eq!(U256Owned::from(&fixed_ref).as_bytes(), &[12u8; 32]);

        let mut fixed_mut_ref = [13u8; 32];
        assert_eq!(U256Owned::from(&mut fixed_mut_ref).as_bytes(), &[13u8; 32]);

        let variable = [14u8; 32];
        assert_eq!(
            B032Owned::try_from(variable).unwrap().as_bytes(),
            &[14u8; 32]
        );

        let variable_ref = [15u8; 32];
        assert_eq!(
            B032Owned::try_from(&variable_ref).unwrap().as_bytes(),
            &[15u8; 32]
        );

        let mut variable_mut_ref = [16u8; 32];
        assert_eq!(
            B032Owned::try_from(&mut variable_mut_ref)
                .unwrap()
                .as_bytes(),
            &[16u8; 32]
        );
    }

    #[test]
    fn borrowed_and_owned_inner_compare_by_payload() {
        let fixed = [17u8; 32];
        let borrowed_fixed = U256::from(&fixed);
        let owned_fixed = U256Owned::from(fixed);
        assert_eq!(borrowed_fixed, owned_fixed);
        assert_eq!(owned_fixed, borrowed_fixed);

        let different_fixed = U256Owned::from([18u8; 32]);
        assert_ne!(borrowed_fixed, different_fixed);

        let variable = [19u8; 32];
        let borrowed_variable = B032::try_from(&variable).unwrap();
        let owned_variable = B032Owned::try_from(variable).unwrap();
        assert_eq!(borrowed_variable, owned_variable);
        assert_eq!(owned_variable, borrowed_variable);

        let different_variable = B032Owned::try_from([20u8; 32]).unwrap();
        assert_ne!(borrowed_variable, different_variable);
    }

    #[test]
    fn invalid_type_configurations_are_rejected_at_construction() {
        assert!(catch_unwind(|| {
            let _ = InnerOwned::<true, 1, 1, 0>::new(vec![0]);
        })
        .is_err());

        assert!(catch_unwind(|| {
            let _ = InnerOwned::<false, 0, 0, 255>::new(vec![1, 2, 3]);
        })
        .is_err());

        assert!(catch_unwind(|| {
            let _ = InnerOwned::<false, 1, 1, 1000>::new(vec![0]);
        })
        .is_err());
    }

    #[test]
    fn invalid_type_configurations_are_rejected_at_read_and_write_boundaries() {
        type InvalidVariable<'a> = Inner<'a, false, 0, 0, 255>;

        assert!(catch_unwind(|| {
            let _ = <InvalidVariable<'_> as SizeHint>::size_hint(&[], 0);
        })
        .is_err());

        let invalid_fixed = InnerOwned::<true, 1, 1, 0> { data: vec![0] };
        assert!(catch_unwind(move || {
            let mut dst = [0_u8; 2];
            let _ = super::write_inner_to_slice(&invalid_fixed, &mut dst);
        })
        .is_err());
    }

    // Both directions of a fixed-size length mismatch must yield
    // `ValueExceedsMaxSize`, matching the pre-split `Inner` behavior.
    #[test]
    fn fixed_payload_length_mismatch_errors_are_precise() {
        let too_short = InnerOwned::<true, 4, 0, 0>::new(vec![0; 3]);
        assert!(matches!(
            too_short,
            Err(Error::ValueExceedsMaxSize(true, 4, 0, 0, _, 3))
        ));

        let too_long = InnerOwned::<true, 4, 0, 0>::new(vec![0; 5]);
        assert!(matches!(
            too_long,
            Err(Error::ValueExceedsMaxSize(true, 4, 0, 0, _, 5))
        ));
    }

    #[test]
    fn variable_payload_length_must_fit_header() {
        assert!(InnerOwned::<false, 1, 1, 255>::new(vec![0; 255]).is_ok());
        assert!(InnerOwned::<false, 1, 1, 255>::new(vec![0; 256]).is_err());
    }

    #[test]
    fn fixed_zero_length_size_hint_is_allowed() {
        type FixedZero<'a> = Inner<'a, true, 0, 0, 0>;

        assert_eq!(<FixedZero<'_> as SizeHint>::size_hint(&[], 0).unwrap(), 0);
    }
}

//! Serde serializer/deserializer for [stratum v2][Sv2] implemented following [serde tutorial][tutorial]
//!
//! ```txt
//! SERDE    <-> Sv2
//! bool     <-> BOOL
//! u8       <-> U8
//! u16      <-> U16
//! U24      <-> U24
//! u32      <-> u32
//! U256     <-> U256
//! String   <-> STRO_255
//! Signature<-> SIGNATURE
//! BO255    <-> BO_255
//! BO64K    <-> BO_64K
//! BO16M    <-> BO_16M
//! [u8]     <-> BYTES
//! Pubkey   <-> PUBKEY
//! Seq0255  <-> SEQ0_255
//! Seq064K  <-> SEQ0_64K
//! ```
//!
//! [Sv2]: https://docs.google.com/document/d/1FadCWj-57dvhxsnFM_7X806qyvhR0u3i85607bGHxvg/edit
//! [tutorial]: https://serde.rs/data-format.html
//!
//!

mod de;
mod error;
mod ser;
pub mod sv2_primitives;

pub use error::{Error, Result};
pub use ser::{to_bytes, to_writer, Serializer};

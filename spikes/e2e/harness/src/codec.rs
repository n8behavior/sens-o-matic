//! The byte layout of the harness: a writer, a reader and the error of a read.
//!
//! The layout is fixed-width and canonical: a number is big-endian, a byte
//! string has a `u32` length in front, and a reader refuses bytes left over, so
//! one value has exactly one encoding. That matters for the signed bytes of an
//! event, which are never encoded again (see [`crate::event`]).

use thiserror::Error;

/// Why a read of bytes failed.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum DecodeError {
    /// The bytes end before the value does.
    #[error("the bytes end before the value does (see the module docs of e2e_harness::event)")]
    Truncated,
    /// The value ends before the bytes do.
    #[error("{0} bytes are left after the value (see the module docs of e2e_harness::event)")]
    Trailing(usize),
    /// A tag names no variant of `what`.
    #[error("tag {tag} names no {what} (see the module docs of e2e_harness::event)")]
    UnknownTag {
        /// The kind of value that was read.
        what: &'static str,
        /// The tag that was found.
        tag: u8,
    },
    /// The format version is not the one of this build.
    #[error(
        "format version {0} is not the version of this build (see the module docs of e2e_harness::event)"
    )]
    UnknownVersion(u8),
    /// A value is out of range for its type.
    #[error("{0} (see the module docs of e2e_harness::event)")]
    Invalid(&'static str),
}

/// Appends values to a buffer.
#[derive(Debug, Default)]
pub(crate) struct Writer(Vec<u8>);

impl Writer {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn u8(&mut self, value: u8) -> &mut Self {
        self.0.push(value);
        self
    }

    pub(crate) fn u32(&mut self, value: u32) -> &mut Self {
        self.0.extend_from_slice(&value.to_be_bytes());
        self
    }

    pub(crate) fn u64(&mut self, value: u64) -> &mut Self {
        self.0.extend_from_slice(&value.to_be_bytes());
        self
    }

    /// Bytes of a known width, with no length.
    pub(crate) fn raw(&mut self, bytes: &[u8]) -> &mut Self {
        self.0.extend_from_slice(bytes);
        self
    }

    /// Bytes of any width, with a `u32` length in front.
    ///
    /// # Panics
    ///
    /// Panics when the bytes are longer than `u32::MAX`, which no event of a
    /// simulation is.
    pub(crate) fn bytes(&mut self, bytes: &[u8]) -> &mut Self {
        let len = u32::try_from(bytes.len()).expect("a field of an event is shorter than 4 GiB");
        self.u32(len).raw(bytes)
    }

    pub(crate) fn finish(self) -> Vec<u8> {
        self.0
    }
}

/// Reads values from a slice.
#[derive(Debug)]
pub(crate) struct Reader<'a> {
    rest: &'a [u8],
}

impl<'a> Reader<'a> {
    pub(crate) fn new(bytes: &'a [u8]) -> Self {
        Self { rest: bytes }
    }

    fn take(&mut self, len: usize) -> Result<&'a [u8], DecodeError> {
        if self.rest.len() < len {
            return Err(DecodeError::Truncated);
        }
        let (head, rest) = self.rest.split_at(len);
        self.rest = rest;
        Ok(head)
    }

    pub(crate) fn array<const N: usize>(&mut self) -> Result<[u8; N], DecodeError> {
        let mut out = [0u8; N];
        out.copy_from_slice(self.take(N)?);
        Ok(out)
    }

    pub(crate) fn u8(&mut self) -> Result<u8, DecodeError> {
        Ok(self.array::<1>()?[0])
    }

    pub(crate) fn u32(&mut self) -> Result<u32, DecodeError> {
        Ok(u32::from_be_bytes(self.array()?))
    }

    pub(crate) fn u64(&mut self) -> Result<u64, DecodeError> {
        Ok(u64::from_be_bytes(self.array()?))
    }

    pub(crate) fn bytes(&mut self) -> Result<Vec<u8>, DecodeError> {
        let len = self.u32()? as usize;
        Ok(self.take(len)?.to_vec())
    }

    /// Succeed only when every byte was read.
    pub(crate) fn finish(self) -> Result<(), DecodeError> {
        if self.rest.is_empty() {
            Ok(())
        } else {
            Err(DecodeError::Trailing(self.rest.len()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_value_has_one_encoding() {
        let mut w = Writer::new();
        w.u8(7).u32(9).u64(11).bytes(b"hi");
        let bytes = w.finish();

        let mut r = Reader::new(&bytes);
        assert_eq!(r.u8(), Ok(7));
        assert_eq!(r.u32(), Ok(9));
        assert_eq!(r.u64(), Ok(11));
        assert_eq!(r.bytes(), Ok(b"hi".to_vec()));
        assert_eq!(r.finish(), Ok(()));
    }

    #[test]
    fn a_reader_refuses_a_short_or_a_long_input() {
        assert_eq!(
            Reader::new(&[0, 0, 0, 5, 1]).bytes(),
            Err(DecodeError::Truncated)
        );
        let mut r = Reader::new(&[1, 2]);
        r.u8().unwrap();
        assert_eq!(r.finish(), Err(DecodeError::Trailing(1)));
    }
}

//! The byte layout of the materials and of the exported state.
//!
//! Numbers are big-endian and every field has a known width, so no length is
//! written; the one open-ended field of a message comes last and runs to the end.
//! A reader refuses bytes that are missing and bytes that are left over, so one
//! value has one encoding. The layouts themselves are in [`crate::material`].

use std::fmt;

/// Why a read failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WireError {
    /// The bytes end before the value does.
    Truncated,
    /// The value ends before the bytes do.
    Trailing(usize),
}

impl fmt::Display for WireError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Truncated => f.write_str("the bytes end before the value does"),
            Self::Trailing(left) => write!(f, "{left} bytes are left after the value"),
        }
    }
}

/// Appends fields to a buffer.
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

    pub(crate) fn raw(&mut self, bytes: &[u8]) -> &mut Self {
        self.0.extend_from_slice(bytes);
        self
    }

    pub(crate) fn finish(self) -> Vec<u8> {
        self.0
    }
}

/// Reads fields from a slice.
#[derive(Debug)]
pub(crate) struct Reader<'a>(&'a [u8]);

impl<'a> Reader<'a> {
    pub(crate) fn new(bytes: &'a [u8]) -> Self {
        Self(bytes)
    }

    pub(crate) fn array<const N: usize>(&mut self) -> Result<[u8; N], WireError> {
        let (head, rest) = self
            .0
            .split_first_chunk::<N>()
            .ok_or(WireError::Truncated)?;
        self.0 = rest;
        Ok(*head)
    }

    pub(crate) fn u8(&mut self) -> Result<u8, WireError> {
        Ok(self.array::<1>()?[0])
    }

    pub(crate) fn u32(&mut self) -> Result<u32, WireError> {
        Ok(u32::from_be_bytes(self.array()?))
    }

    /// Everything that is left: the open-ended field at the end of a message.
    pub(crate) fn rest(&mut self) -> &'a [u8] {
        std::mem::take(&mut self.0)
    }

    /// Succeed only when every byte was read.
    pub(crate) fn finish(self) -> Result<(), WireError> {
        if self.0.is_empty() {
            Ok(())
        } else {
            Err(WireError::Trailing(self.0.len()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_value_has_one_encoding() {
        let mut w = Writer::new();
        w.u8(7).u32(9).raw(b"tail");
        let bytes = w.finish();

        let mut r = Reader::new(&bytes);
        assert_eq!(r.u8(), Ok(7));
        assert_eq!(r.u32(), Ok(9));
        assert_eq!(r.rest(), b"tail");
        assert_eq!(r.finish(), Ok(()));
    }

    #[test]
    fn a_reader_refuses_a_short_or_a_long_input() {
        assert_eq!(Reader::new(&[0, 0, 1]).u32(), Err(WireError::Truncated));
        let mut r = Reader::new(&[1, 2]);
        r.u8().unwrap();
        assert_eq!(r.finish(), Err(WireError::Trailing(1)));
    }
}

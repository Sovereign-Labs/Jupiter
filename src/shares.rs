use std::fmt::Display;

use prost::{bytes::Buf, encoding::decode_varint};
use sovereign_sdk::{
    core::crypto::hash::{sha2, Sha2Hash},
    Bytes,
};

use crate::skip_varint;

/// The size of a share, in bytes
const SHARE_SIZE: usize = 512;
/// The length of a namespace, in bytes,
const NAMESPACE_LEN: usize = 8;
/// Value of maximum reserved namespace ID, as a big endian integer
const MAX_RESERVED_NAMESPACE_ID: u64 = 255;

/// A group of shares, in a single namespace
pub enum NamespaceGroup {
    Compact(Vec<Share>),
    Sparse(Vec<Share>),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Share {
    Continuation(Bytes),
    Start(Bytes),
}

impl AsRef<[u8]> for Share {
    fn as_ref(&self) -> &[u8] {
        self.raw_inner_ref()
    }
}

fn is_continuation_unchecked(share: &[u8]) -> bool {
    share[8] & 0x01 == 0
}

fn enforce_version_zero(share: &[u8]) {
    assert!(share[8] & !0x01 == 0)
}

#[derive(Debug, Clone, PartialEq, Copy)]
pub enum ShareError {
    NotAStartShare,
    InvalidEncoding,
}

impl Share {
    pub fn new(inner: Bytes) -> Self {
        enforce_version_zero(inner.as_ref());
        if is_continuation_unchecked(inner.as_ref()) {
            Self::Continuation(inner)
        } else {
            Self::Start(inner)
        }
    }

    pub fn is_sequence_start(&self) -> bool {
        match self {
            Share::Continuation(_) => false,
            Share::Start(_) => true,
        }
    }

    pub fn sequence_length(&self) -> Result<u64, ShareError> {
        match self {
            Share::Continuation(_) => Err(ShareError::NotAStartShare),
            Share::Start(inner) => {
                let mut inner = inner.clone();
                inner.advance(9);
                return decode_varint(&mut inner).map_err(|_| ShareError::InvalidEncoding);
            }
        }
    }

    /// Returns this share in raw serialized form as Bytes
    fn raw_inner(&self) -> Bytes {
        match self {
            Share::Continuation(inner) => inner.clone(),
            Share::Start(inner) => inner.clone(),
        }
    }

    /// Returns this share in raw serialized form as a slice
    fn raw_inner_ref(&self) -> &[u8] {
        match self {
            Share::Continuation(inner) => inner.as_ref(),
            Share::Start(inner) => inner.as_ref(),
        }
    }

    pub fn hash(&self) -> Sha2Hash {
        sha2(self.raw_inner_ref())
    }

    fn get_data_offset(&self) -> usize {
        // All shares are prefixed with metadata including the namespace (8 bytes), and info byte (1 byte)
        let mut offset = 8 + 1;
        // Compact shares (shares in reserved namespaces) are prefixed with 2 reserved bytes
        if u64::from_be_bytes(self.namespace()) <= 255 {
            offset += 2;
            // Start shares in compact namespaces are also prefixed with a sequence length, which
            // is zero padded to fill four bytes
            if let Self::Start(_) = self {
                offset += 4
            }
        } else {
            // Start shares in sparse namespaces are prefixed with a sequence length, which
            // is encoded as a regular varint
            if let Self::Start(_) = self {
                offset += skip_varint(&self.raw_inner_ref()[offset..])
                    .expect("Share must be validly encoded")
            }
        }
        offset
    }

    /// Returns the data of this share as a `Bytes`
    pub fn data(&self) -> Bytes {
        let mut output = self.raw_inner();
        output.advance(self.get_data_offset());
        output
    }

    /// Returns the data of this share as &[u8]
    pub fn data_ref(&self) -> &[u8] {
        &self.raw_inner_ref()[self.get_data_offset()..]
    }

    /// Get the namespace associated with this share
    pub fn namespace(&self) -> [u8; NAMESPACE_LEN] {
        let mut out = [0u8; 8];
        out.copy_from_slice(&self.raw_inner_ref()[..8]);
        out
    }
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum ShareParsingError {
    ErrInvalidBase64,
    ErrWrongLength,
}

impl Display for ShareParsingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ShareParsingError::ErrInvalidBase64 => {
                f.write_str("ShareParsingError::ErrInvalidBase64")
            }
            ShareParsingError::ErrWrongLength => f.write_str("ShareParsingError::ErrWrongLength"),
        }
    }
}

impl std::error::Error for ShareParsingError {}

impl NamespaceGroup {
    pub fn from_b64(b64: &str) -> Result<Self, ShareParsingError> {
        let mut decoded = Vec::with_capacity((b64.len() + 3) / 4 * 3);
        // unsafe { decoded.set_len((b64.len() / 4 * 3)) }
        if let Err(_) = base64::decode_config_buf(b64, base64::STANDARD, &mut decoded) {
            return Err(ShareParsingError::ErrInvalidBase64);
        }
        let mut output: Bytes = decoded.into();
        if output.len() % SHARE_SIZE != 0 {
            println!(
                "Wrong length: Expected a multiple of 512, got: {}",
                output.len()
            );
            return Err(ShareParsingError::ErrWrongLength);
        }
        let mut shares = Vec::with_capacity((output.len() / 512) + 1);
        while output.len() > SHARE_SIZE {
            shares.push(Share::new(output.split_to(SHARE_SIZE)));
        }
        shares.push(Share::new(output));
        // Check whether these shares come from a reserved (compact) namespace

        let namespace = shares[0].namespace();
        if u64::from_be_bytes(namespace) <= MAX_RESERVED_NAMESPACE_ID {
            Ok(Self::Compact(shares))
        } else {
            Ok(Self::Sparse(shares))
        }
    }

    pub fn shares(&self) -> &Vec<Share> {
        match self {
            NamespaceGroup::Compact(shares) => shares,
            NamespaceGroup::Sparse(shares) => shares,
        }
    }

    pub fn blobs<'a>(&self) -> NamespaceIterator {
        NamespaceIterator {
            offset: 0,
            shares: self,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Blob<'a>(pub &'a [Share]);

impl<'a> Blob<'a> {
    pub fn with(shares: &'a [Share]) -> Self {
        Self(shares)
    }

    pub fn data(&self) -> BlobIterator {
        let sequence_length = self.0[0]
            .sequence_length()
            .expect("blob must contain start share at idx 0");
        BlobIterator {
            sequence_len: sequence_length as usize,
            consumed: 0,
            current: self.0[0].data(),
            current_idx: 0,
            shares: self.0,
        }
    }
}

pub struct BlobIterator<'a> {
    sequence_len: usize,
    consumed: usize,
    current: Bytes,
    current_idx: usize,
    shares: &'a [Share],
}

impl<'a> Iterator for BlobIterator<'a> {
    type Item = u8;

    fn next(&mut self) -> Option<Self::Item> {
        if self.consumed == self.sequence_len {
            return None;
        }
        if self.current.has_remaining() {
            self.consumed += 1;
            return Some(self.current.get_u8());
        }
        self.current_idx += 1;
        self.current = self.shares[self.current_idx].data();
        self.next()
    }
}

impl<'a> Buf for BlobIterator<'a> {
    fn remaining(&self) -> usize {
        self.sequence_len - self.consumed
    }

    fn chunk(&self) -> &[u8] {
        let chunk = if self.current.has_remaining() {
            self.current.as_ref()
        } else {
            // If the current share is exhasted, try to take the data from the next one
            // if there is no next chunk, we're done. Return the empty slice.
            if self.current_idx + 1 >= self.shares.len() {
                return &[];
            }
            // Otherwise, take the next chunk
            self.shares[self.current_idx + 1].data_ref()
        };
        // Chunks are zero-padded, so truncate if necessary
        let remaining = self.remaining();
        if chunk.len() > remaining {
            return &chunk[..remaining];
        }
        chunk
    }

    fn advance(&mut self, cnt: usize) {
        self.consumed += cnt;
        if self.current.remaining() > cnt {
            self.current.advance(cnt);
            return;
        }

        let next_cnt = cnt - self.current.remaining();
        self.current_idx += 1;
        self.current = self.shares[self.current_idx].data();
        self.current.advance(next_cnt);
    }
}

pub struct NamespaceIterator<'a> {
    offset: usize,
    shares: &'a NamespaceGroup,
}

impl<'a> std::iter::Iterator for NamespaceIterator<'a> {
    type Item = Blob<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.offset == self.shares.shares().len() {
            return None;
        }
        match self.shares {
            NamespaceGroup::Compact(shares) => {
                self.offset = shares.len();
                Some(Blob::with(shares))
            }
            NamespaceGroup::Sparse(shares) => {
                let start = self.offset;
                self.offset += 1;

                if self.offset == shares.len() {
                    return Some(Blob::with(&shares[start..self.offset]));
                }

                for (idx, share) in shares[self.offset..].iter().enumerate() {
                    if share.is_sequence_start() {
                        self.offset += idx;
                        return Some(Blob::with(&shares[start..self.offset]));
                    }
                }

                self.offset = shares.len();
                return Some(Blob::with(&shares[start..self.offset]));
            }
        }
        // let start = self.offset;
        // let length = 0;
        // loop {

        // }
    }
}

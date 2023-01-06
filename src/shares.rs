use std::fmt::Display;

use base64::STANDARD;
use nmt_rs::NamespaceId;
use prost::{
    bytes::{Buf, BytesMut},
    encoding::decode_varint,
    DecodeError,
};
use serde::{de::Error, Deserialize};
use sovereign_sdk::{
    core::crypto::hash::{sha2, Sha2Hash},
    Bytes,
};

use crate::da_service::TRANSACTIONS_NAMESPACE;

/// Skip over a varint. Returns the number of bytes read
pub fn skip_varint(mut bytes: impl Buf) -> Result<usize, ErrInvalidVarint> {
    // A varint may contain up to 10 bytes
    for i in 0..10 {
        // If the continuation bit is not set, we're done
        if bytes.get_u8() < 0x80 {
            return Ok(i + 1);
        }
    }
    Err(ErrInvalidVarint)
}

/// Read a varint. Returns the value (as a u64) and the number of bytes read
pub fn read_varint(mut bytes: impl Buf) -> Result<(u64, usize), DecodeError> {
    let original_len = bytes.remaining();
    let varint = decode_varint(&mut bytes)?;
    Ok((varint, original_len - bytes.remaining()))
}

#[derive(Debug, PartialEq)]
pub struct ErrInvalidVarint;

/// The size of a share, in bytes
const SHARE_SIZE: usize = 512;
/// The size of base64 encoded share, in bytes
const B64_SHARE_SIZE: usize = 684;

#[derive(Debug, Clone, PartialEq, serde::Serialize, Deserialize)]
/// A group of shares, in a single namespace
pub enum NamespaceGroup {
    Compact(Vec<Share>),
    Sparse(Vec<Share>),
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub enum Share {
    Continuation(Bytes),
    Start(Bytes),
}

impl<'de> Deserialize<'de> for Share {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let mut share = Bytes::deserialize(deserializer)?;
        if share.len() == B64_SHARE_SIZE {
            let mut decoded = BytesMut::with_capacity(SHARE_SIZE);
            unsafe { decoded.set_len(SHARE_SIZE) }
            base64::decode_config_slice(share, STANDARD, &mut decoded[..])
                .map_err(|_| Error::custom("Invalid base64 encoding"))?;
            share = decoded.freeze()
        }
        if share.len() != SHARE_SIZE {
            // let expected = Unexpected::Bytes(&share);
            return Err(Error::invalid_length(share.len(), &"A share of length 512"));
        }
        if is_continuation_unchecked(share.as_ref()) {
            return Ok(Share::Continuation(share));
        }
        Ok(Share::Start(share))
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

    pub fn as_serialized(&self) -> &[u8] {
        self.raw_inner_ref()
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

    /// Returns the offset *into the data portion* of this share at which
    /// the first tx begins. So, for example, if this share is the start of a sequence,
    /// the returned offset will be 0.
    fn offset_of_first_tx_unchecked(&self) -> usize {
        let offset = self.get_data_offset() - 2;
        decode_varint(&mut std::io::Cursor::new(&self.raw_inner_ref()[offset..]))
            .expect("reserved bytes must be valid varint") as usize
            - self.get_data_offset()
    }

    fn get_data_offset(&self) -> usize {
        // All shares are prefixed with metadata including the namespace (8 bytes), and info byte (1 byte)
        let mut offset = 8 + 1;
        // Compact shares (shares in reserved namespaces) are prefixed with 2 reserved bytes
        if self.namespace().is_reserved() {
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
    pub fn namespace(&self) -> NamespaceId {
        let mut out = [0u8; 8];
        out.copy_from_slice(&self.raw_inner_ref()[..8]);
        NamespaceId(out)
    }

    pub fn is_valid_tx_start(&self, idx: usize) -> bool {
        if self.namespace() != TRANSACTIONS_NAMESPACE {
            return false;
        }
        let mut next_legal_start_offset = self.offset_of_first_tx_unchecked();
        let mut remaining_data = self.data();
        loop {
            if next_legal_start_offset == idx {
                return true;
            }
            if let Ok((tx_len, len_of_len)) = read_varint(&mut remaining_data) {
                next_legal_start_offset += tx_len as usize + len_of_len;
                remaining_data.advance(tx_len as usize);
            } else {
                return false;
            }
        }
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

        if shares[0].namespace().is_reserved() {
            Ok(Self::Compact(shares))
        } else {
            Ok(Self::Sparse(shares))
        }
    }

    pub fn from_b64_shares(encoded_shares: &Vec<String>) -> Result<Self, ShareParsingError> {
        if encoded_shares.len() == 0 {
            return Ok(Self::Sparse(vec![]));
        }
        let mut shares = Vec::with_capacity(encoded_shares.len());
        for share in encoded_shares {
            let decoded_vec =
                base64::decode(share).map_err(|_| ShareParsingError::ErrInvalidBase64)?;
            if decoded_vec.len() != 512 {
                return Err(ShareParsingError::ErrWrongLength);
            }
            let share = Share::new(decoded_vec.into());
            shares.push(share)
        }

        if shares[0].namespace().is_reserved() {
            Ok(Self::Compact(shares))
        } else {
            Ok(Self::Sparse(shares))
        }
    }

    // Panics if less than 1 share is provided
    pub fn from_shares_unchecked(shares: Vec<Vec<u8>>) -> Self {
        let shares: Vec<Share> = shares
            .into_iter()
            .map(|share| Share::new(Bytes::from(share)))
            .collect();

        if shares[0].namespace().is_reserved() {
            Self::Compact(shares)
        } else {
            Self::Sparse(shares)
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
pub struct Blob(pub Vec<Share>);

impl<'a> From<BlobRef<'a>> for Blob {
    fn from(value: BlobRef<'a>) -> Self {
        Self(value.0.iter().map(|s| s.clone()).collect())
    }
}

impl IntoIterator for Blob {
    type Item = u8;

    type IntoIter = BlobIterator;

    fn into_iter(self) -> Self::IntoIter {
        let sequence_length = self.0[0]
            .sequence_length()
            .expect("blob must contain start share at idx 0");
        BlobIterator {
            sequence_len: sequence_length as usize,
            consumed: 0,
            current: self.0[0].data(),
            current_idx: 0,
            blob: self,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct BlobRef<'a>(pub &'a [Share]);

impl<'a> BlobRef<'a> {
    pub fn with(shares: &'a [Share]) -> Self {
        Self(shares)
    }

    pub fn data(&self) -> BlobRefIterator {
        let sequence_length = self.0[0]
            .sequence_length()
            .expect("blob must contain start share at idx 0");
        BlobRefIterator {
            sequence_len: sequence_length as usize,
            consumed: 0,
            current: self.0[0].data(),
            current_idx: 0,
            shares: self.0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct BlobIterator {
    sequence_len: usize,
    consumed: usize,
    current: Bytes,
    current_idx: usize,
    blob: Blob,
}

impl Iterator for BlobIterator {
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
        self.current = self.blob.0[self.current_idx].data();
        self.next()
    }
}

impl Buf for BlobIterator {
    fn remaining(&self) -> usize {
        self.sequence_len - self.consumed
    }

    fn chunk(&self) -> &[u8] {
        let chunk = if self.current.has_remaining() {
            self.current.as_ref()
        } else {
            // If the current share is exhasted, try to take the data from the next one
            // if there is no next chunk, we're done. Return the empty slice.
            if self.current_idx + 1 >= self.blob.0.len() {
                return &[];
            }
            // Otherwise, take the next chunk
            self.blob.0[self.current_idx + 1].data_ref()
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
        self.current = self.blob.0[self.current_idx].data();
        self.current.advance(next_cnt);
    }
}

#[derive(Debug, Clone)]
pub struct BlobRefIterator<'a> {
    sequence_len: usize,
    consumed: usize,
    current: Bytes,
    current_idx: usize,
    shares: &'a [Share],
}

impl<'a> BlobRefIterator<'a> {
    pub fn current_position(&self) -> (usize, usize) {
        (
            self.current_idx,
            self.shares[self.current_idx].data_ref().len() - self.current.remaining(),
        )
    }
}

impl<'a> Iterator for BlobRefIterator<'a> {
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

impl<'a> Buf for BlobRefIterator<'a> {
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
    type Item = BlobRef<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.offset == self.shares.shares().len() {
            return None;
        }
        match self.shares {
            NamespaceGroup::Compact(shares) => {
                self.offset = shares.len();
                Some(BlobRef::with(shares))
            }
            NamespaceGroup::Sparse(shares) => {
                let start = self.offset;
                self.offset += 1;

                if self.offset == shares.len() {
                    return Some(BlobRef::with(&shares[start..self.offset]));
                }

                for (idx, share) in shares[self.offset..].iter().enumerate() {
                    if share.is_sequence_start() {
                        self.offset += idx;
                        return Some(BlobRef::with(&shares[start..self.offset]));
                    }
                }

                self.offset = shares.len();
                return Some(BlobRef::with(&shares[start..self.offset]));
            }
        }
        // let start = self.offset;
        // let length = 0;
        // loop {

        // }
    }
}

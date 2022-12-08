use std::io::Cursor;

use hex_literal::hex;
use prost::bytes::Buf;
use serde::{Deserialize, Serialize};
use sovereign_sdk::core::traits::{Address, Blockheader, CanonicalHash};

use crate::{MalleatedTx, Tx};

#[derive(Deserialize, Serialize, PartialEq, Debug, Clone)]
pub struct CelestiaHeaderResponse {
    header: CelestiaHeader,
}

#[derive(Deserialize, Serialize, PartialEq, Debug, Clone)]
/// The minimal portion of a celestia header required for DA verification
pub struct CelestiaHeader {
    pub version: CelestiaVersion,
    pub chain_id: String,
    pub height: u64,
    pub time: String, // TODO: Make this an actual time
    pub last_block_id: PreviousBlock,
    pub last_commit_hash: Sha2Hash,
    pub data_hash: Sha2Hash,
    pub consensus_hash: Sha2Hash,
    pub app_hash: Sha2Hash,
}

impl Blockheader for CelestiaHeader {
    type Hash = Sha2Hash;

    fn prev_hash(&self) -> &Self::Hash {
        &self.last_block_id.hash
    }
}

impl CanonicalHash for CelestiaHeader {
    type Output = Sha2Hash;

    fn hash(&self) -> Self::Output {
        todo!()
    }
}

#[derive(Deserialize, Serialize, PartialEq, Debug, Clone)]
pub struct CelestiaVersion {
    pub block: u32,
}

#[derive(Deserialize, Serialize, PartialEq, Debug, Clone)]
pub struct PreviousBlock {
    pub hash: Sha2Hash,
    // TODO: add parts
}

#[derive(Deserialize, Serialize, PartialEq, Debug, Clone)]
pub struct Sha2Hash(#[serde(deserialize_with = "hex::deserialize")] pub [u8; 32]);

impl AsRef<[u8]> for Sha2Hash {
    fn as_ref(&self) -> &[u8] {
        self.0.as_ref()
    }
}

#[derive(Deserialize, Serialize, PartialEq, Debug, Clone)]
pub struct H160(#[serde(deserialize_with = "hex::deserialize")] pub [u8; 20]);

impl AsRef<[u8]> for H160 {
    fn as_ref(&self) -> &[u8] {
        self.0.as_ref()
    }
}

pub struct NotExactlyTwentyBytes;

impl<'a> TryFrom<&'a [u8]> for H160 {
    type Error = NotExactlyTwentyBytes;

    fn try_from(value: &'a [u8]) -> Result<Self, Self::Error> {
        if value.len() == 20 {
            let mut addr = [0u8; 20];
            addr.copy_from_slice(value);
            return Ok(Self(addr));
        }
        Err(NotExactlyTwentyBytes)
    }
}
impl Address for H160 {}

// pub fn skip_share_header(mut bytes: impl Buf) {
//     // Skip the namespace id
//     bytes.advance(8);
//     // Read the info byte
//     let info = bytes.get_u8();
//     let is_sequence_start = info & 0x01 == 1;
//     if is_sequence_start {
//         // // Skip sequence length
//         // let used_seq_len = skip_varint(&mut bytes).expect("encoding must be valid");
//         // if used_seq_len < 2 {
//         //     bytes.advance(2 - used_seq_len)
//         // }
//         bytes.advance(4);
//     }
//     // Skip reserved bytes
//     bytes.advance(2);
//     // skip_varint(&mut bytes).expect("encoding must be valid");
//     // skip the length field
//     skip_varint(bytes).expect("encoding must be valid");
// }

pub fn skip_compact_share_header(mut bytes: impl Buf) {
    // Skip the namespace id
    bytes.advance(8);
    // Read the info byte
    let info = bytes.get_u8();
    let is_sequence_start = info & 0x01 == 1;
    if is_sequence_start {
        // Skip sequence length
        // bytes.advance(4);
    }
    // Skip reserved bytes
    bytes.advance(2);
}

pub fn test_compact_share_parsing(mut bytes: impl Buf) {
    skip_compact_share_header(&mut bytes);

    // read the length field
    let (tx_len, _) = decode_varint(bytes)
        .expect("encoding must be valid")
        .unwrap();

    println!("Tx len was {}", tx_len);
    if tx_len >= 490 {
        panic!("Tx len was larger than expected: {}", tx_len)
    }
}

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

#[derive(Debug, PartialEq)]
pub struct ErrInvalidVarint;

pub fn decode_varint(mut rem: impl Buf) -> Result<Option<(u64, usize)>, ErrInvalidVarint> {
    if !rem.has_remaining() {
        return Ok(None);
    }
    let mut r: u64 = 0;
    for i in 0..9 {
        let b = rem.get_u8();
        r = r | (((b & 0x7f) as u64) << (i as u64 * 7));
        if b < 0x80 {
            return Ok(Some((r, i + 1)));
        }
    }
    let b = rem.get_u8();
    // By this point, we've parsed 63 bits, so only the lsb of the remaining byte may be set
    if b > 1 {
        return Err(ErrInvalidVarint);
    }
    r |= (b as u64) << 63;
    Ok(Some((r, 10)))
}

#[test]
fn test_decode_varint() {
    assert_eq!(decode_varint(Cursor::new(hex!("9601"))), Ok(Some((150, 2))))
}

#[derive(Debug, PartialEq, Clone)]
pub enum TxType {
    Pfd(MalleatedTx),
    Other(Tx),
}

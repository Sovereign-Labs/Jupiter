use std::ops::Range;

use nmt_rs::NamespacedHash;
use prost::{bytes::Buf, encoding::decode_varint, Message};
use serde::{Deserialize, Serialize};
use sovereign_sdk::core::traits::{Address, Blockheader, CanonicalHash};

const NAMESPACED_HASH_LEN: usize = 48;

use crate::{
    da_app::{address::CelestiaAddress, TmHash},
    da_service::TRANSACTIONS_NAMESPACE,
    payment::MsgPayForData,
    shares::{Blob, BlobRefIterator, NamespaceGroup},
    utils::BoxError,
    MalleatedTx, Tx,
};

#[derive(Deserialize, Serialize, PartialEq, Debug, Clone)]
pub struct MarshalledDataAvailabilityHeader {
    pub row_roots: Vec<String>,
    pub column_roots: Vec<String>,
}

#[derive(PartialEq, Debug, Clone, Deserialize, serde::Serialize)]
pub struct DataAvailabilityHeader {
    pub row_roots: Vec<NamespacedHash>,
    pub column_roots: Vec<NamespacedHash>,
}

// Danger! This method panics if the provided bas64 is longer than a namespaced hash
fn decode_to_ns_hash(b64: &str) -> Result<NamespacedHash, base64::DecodeError> {
    let mut out = [0u8; NAMESPACED_HASH_LEN];
    base64::decode_config_slice(b64, base64::STANDARD, &mut out)?;
    Ok(NamespacedHash(out))
}

impl TryFrom<MarshalledDataAvailabilityHeader> for DataAvailabilityHeader {
    type Error = base64::DecodeError;

    fn try_from(value: MarshalledDataAvailabilityHeader) -> Result<Self, Self::Error> {
        let mut row_roots = Vec::with_capacity(value.row_roots.len());
        for root in value.row_roots {
            row_roots.push(decode_to_ns_hash(&root)?);
        }
        let mut column_roots = Vec::with_capacity(value.column_roots.len());
        for root in value.column_roots {
            column_roots.push(decode_to_ns_hash(&root)?);
        }
        Ok(Self {
            row_roots,
            column_roots,
        })
    }
}

#[derive(Deserialize, Serialize, PartialEq, Debug, Clone)]
pub struct CelestiaHeaderResponse {
    pub header: tendermint::block::Header,
    pub dah: MarshalledDataAvailabilityHeader,
}

#[derive(Deserialize, Serialize, PartialEq, Debug, Clone)]
pub struct NamespacedSharesResponse {
    pub shares: Option<Vec<String>>,
    pub height: u64,
}

#[derive(Debug, PartialEq, Clone, Deserialize, serde::Serialize)]
pub struct CelestiaHeader {
    pub dah: DataAvailabilityHeader,
    pub header: tendermint::block::Header,
}

impl CelestiaHeader {
    pub fn square_size(&self) -> usize {
        self.dah.row_roots.len()
    }
}

impl CanonicalHash for CelestiaHeader {
    type Output = TmHash;

    fn hash(&self) -> Self::Output {
        TmHash(self.header.hash())
    }
}

#[derive(PartialEq, Clone, Debug)]
pub struct BlobWithSender {
    pub blob: Blob,
    pub sender: CelestiaAddress,
}

impl Blockheader for CelestiaHeader {
    type Hash = TmHash;

    fn prev_hash(&self) -> &Self::Hash {
        self.header
            .last_block_id
            .as_ref()
            .expect("must not call prev_hash on block with no predecessor")
            .hash
            .as_ref()
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

#[derive(Debug, PartialEq, Clone)]
pub enum TxType {
    Pfd(MalleatedTx),
    Other(Tx),
}

pub fn parse_tx_namespace(
    group: NamespaceGroup,
) -> Result<Vec<(MsgPayForData, TxPosition)>, BoxError> {
    if group.shares().len() == 0 {
        return Ok(vec![]);
    }
    assert!(group.shares()[0].namespace() == TRANSACTIONS_NAMESPACE);
    let mut pfbs = Vec::new();
    for blob in group.blobs() {
        let mut data = blob.data();
        println!("Total Data length: {}", data.remaining());

        while data.has_remaining() {
            dbg!(data.remaining());
            if let Some(tx) = next_e_tx(&mut data)? {
                pfbs.push(tx)
            }
        }
    }
    Ok(pfbs)
}

#[derive(Debug, PartialEq, Clone, serde::Serialize, Deserialize)]
pub struct TxPosition {
    /// The half-open range of shares across which this transaction is serialized.
    /// For example a transaction which was split across shares 5,6, and 7 would have range 5..8
    pub share_range: Range<usize>,
    /// The offset into the first share at which the transaction starts
    pub start_offset: usize,
}

fn next_e_tx(
    mut data: &mut BlobRefIterator,
) -> Result<Option<(MsgPayForData, TxPosition)>, BoxError> {
    let (start_idx, start_offset) = data.current_position();
    let len = decode_varint(&mut data).expect("Varint must be valid");
    let backup = data.clone();
    let tx = match MalleatedTx::decode(&mut data) {
        Ok(malleated) => {
            // The hash length must be 32
            if malleated.original_tx_hash.len() != 32 {
                *data = backup;
                TxType::Other(Tx::decode(&mut data)?)
            } else {
                TxType::Pfd(malleated)
            }
        }
        Err(_) => {
            *data = backup;
            data.advance(len as usize);
            return Ok(None);
        }
    };

    let sdk_tx = match tx {
        TxType::Pfd(malleated) => {
            let inner = malleated.tx.clone();
            Tx::decode(inner)?
        }
        TxType::Other(_) => panic!("This tx is unmalleated and should ahve been skipped"),
    };

    let body = sdk_tx.body.expect("transaction must have body");
    if body.messages.len() == 1 {
        for msg in body.messages {
            if msg.type_url != "/payment.MsgPayForData" {
                panic!("This tx is not a pfd and should have been skipped")
            }
            let pfd = MsgPayForData::decode(std::io::Cursor::new(msg.value))?;
            let current_share_idx = data.current_position().0;
            return Ok(Some((
                pfd,
                TxPosition {
                    share_range: start_idx..current_share_idx + 1,
                    start_offset,
                },
            )));
        }
    }
    Ok(None)
}

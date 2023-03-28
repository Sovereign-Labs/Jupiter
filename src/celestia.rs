use std::ops::Range;

use borsh::{BorshDeserialize, BorshSerialize};
use nmt_rs::NamespacedHash;
use prost::{bytes::Buf, Message};
use serde::{Deserialize, Serialize};
use sovereign_sdk::core::traits::{
    AddressTrait as Address, BlockheaderTrait as Blockheader, CanonicalHash,
};
use tracing::debug;

const NAMESPACED_HASH_LEN: usize = 48;

use crate::{
    da_app::{address::CelestiaAddress, TmHash},
    da_service::PFB_NAMESPACE,
    pfb::{BlobTx, MsgPayForBlobs, Tx},
    shares::{read_varint, Blob, BlobRefIterator, NamespaceGroup},
    utils::BoxError,
};

#[derive(Deserialize, Serialize, PartialEq, Debug, Clone)]
pub struct MarshalledDataAvailabilityHeader {
    pub row_roots: Vec<String>,
    pub column_roots: Vec<String>,
}

#[derive(
    PartialEq, Debug, Clone, Deserialize, serde::Serialize, BorshDeserialize, BorshSerialize,
)]
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

#[derive(Deserialize, Serialize, PartialEq, Debug, Clone, Eq)]
pub struct H160(#[serde(deserialize_with = "hex::deserialize")] pub [u8; 20]);

impl AsRef<[u8]> for H160 {
    fn as_ref(&self) -> &[u8] {
        self.0.as_ref()
    }
}


impl<'a> TryFrom<&'a [u8]> for H160 {
    type Error = anyhow::Error;

    fn try_from(value: &'a [u8]) -> Result<Self, Self::Error> {
        if value.len() == 20 {
            let mut addr = [0u8; 20];
            addr.copy_from_slice(value);
            return Ok(Self(addr));
        }
        anyhow::bail!("Adress is not exactly 20 bytes");
    }
}
impl Address for H160 {}

pub fn parse_pfb_namespace(
    group: NamespaceGroup,
) -> Result<Vec<(MsgPayForBlobs, TxPosition)>, BoxError> {
    if group.shares().len() == 0 {
        return Ok(vec![]);
    }
    assert!(group.shares()[0].namespace() == PFB_NAMESPACE);
    let mut pfbs = Vec::new();
    for blob in group.blobs() {
        let mut data = blob.data();
        while data.has_remaining() {
            pfbs.push(next_pfb(&mut data)?)
        }
    }
    Ok(pfbs)
}

#[derive(
    Debug, PartialEq, Clone, serde::Serialize, Deserialize, BorshSerialize, BorshDeserialize,
)]
pub struct TxPosition {
    /// The half-open range of shares across which this transaction is serialized.
    /// For example a transaction which was split across shares 5,6, and 7 would have range 5..8
    pub share_range: Range<usize>,
    /// The offset into the first share at which the transaction starts
    pub start_offset: usize,
}

pub(crate) fn pfb_from_iter(data: impl Buf, pfb_len: usize) -> Result<MsgPayForBlobs, BoxError> {
    debug!("Decoding blob tx");
    let mut blob_tx = BlobTx::decode(data.take(pfb_len))?;
    debug!("Decoding cosmos sdk tx");
    let cosmos_tx = Tx::decode(&mut blob_tx.tx)?;
    let messages = cosmos_tx
        .body
        .ok_or(anyhow::format_err!("No body in cosmos tx"))?
        .messages;
    if messages.len() != 1 {
        return Err(anyhow::format_err!("Expected 1 message in cosmos tx"));
    }
    debug!("Decoding PFB from blob tx value");
    Ok(MsgPayForBlobs::decode(&mut &messages[0].value[..])?)
}

fn next_pfb(mut data: &mut BlobRefIterator) -> Result<(MsgPayForBlobs, TxPosition), BoxError> {
    let (start_idx, start_offset) = data.current_position();
    let (len, len_of_len) = read_varint(&mut data).expect("Varint must be valid");
    debug!(
        "Decoding wrapped PFB of length {}. Stripped {} bytes of prefix metadata",
        len, len_of_len
    );

    let current_share_idx = data.current_position().0;
    let pfb = pfb_from_iter(&mut data, len as usize)?;

    Ok((
        pfb,
        TxPosition {
            share_range: start_idx..current_share_idx + 1,
            start_offset,
        },
    ))
}

use nmt_rs::NamespacedHash;
use prost::{bytes::Buf, encoding::decode_varint, Message};
use serde::{Deserialize, Serialize};
use sovereign_sdk::core::traits::{Address, Blockheader, CanonicalHash};

const NAMESPACED_HASH_LEN: usize = 48;

use crate::{
    da_app::CelestiaAddress,
    payment::MsgPayForData,
    shares::{Blob, NamespaceGroup},
    MalleatedTx, Tx,
};

#[derive(Deserialize, Serialize, PartialEq, Debug, Clone)]
pub struct MarshalledDataAvailabilityHeader {
    pub row_roots: Vec<String>,
    pub column_roots: Vec<String>,
}

#[derive(PartialEq, Debug, Clone)]
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
    pub shares: Vec<String>,
    pub height: u64,
}

// #[derive(Deserialize, Serialize, PartialEq, Debug, Clone)]
// /// The minimal portion of a celestia header required for DA verification
// pub struct CelestiaHeader {
//     pub version: CelestiaVersion,
//     pub chain_id: String,
//     pub height: u64,
//     pub time: String, // TODO: Make this an actual time
//     pub last_block_id: PreviousBlock,
//     pub last_commit_hash: Sha2Hash,
//     pub data_hash: Sha2Hash,
//     pub consensus_hash: Sha2Hash,
//     pub app_hash: Sha2Hash,
// }

#[derive(Debug, PartialEq, Clone)]

pub struct CelestiaHeader {
    pub dah: DataAvailabilityHeader,
    pub header: tendermint::block::Header,
}

impl CanonicalHash for CelestiaHeader {
    type Output = tendermint::Hash;

    fn hash(&self) -> Self::Output {
        self.header.hash()
    }
}

#[derive(PartialEq, Clone, Debug)]
pub struct BlobWithSender {
    pub blob: Blob,
    pub sender: CelestiaAddress,
}

impl Blockheader for CelestiaHeader {
    type Hash = tendermint::Hash;

    fn prev_hash(&self) -> &Self::Hash {
        &self
            .header
            .last_block_id
            .as_ref()
            .expect("must not call prev_hash on block with no predecessor")
            .hash
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

// pub fn get_pfds(height: u64) -> Result<Vec<MsgPayForData>, Box<dyn std::error::Error>> {
//     let e_tx_shares = get_namespace_data(height, [0, 0, 0, 0, 0, 0, 0, 1])?;
//     let pfds = parse_tx_namespace(e_tx_shares)?;
//     Ok(pfds)
// }

pub fn parse_tx_namespace(
    group: NamespaceGroup,
) -> Result<Vec<MsgPayForData>, Box<dyn std::error::Error>> {
    if group.shares().len() == 0 {
        return Ok(vec![]);
    }
    assert!(group.shares()[0].namespace() == [0u8, 0, 0, 0, 0, 0, 0, 1]);
    let mut pfbs = Vec::new();
    for blob in group.blobs() {
        let data: Vec<u8> = blob.data().collect();
        println!("Total Data length: {}", data.len());

        let mut data = std::io::Cursor::new(data);
        while data.has_remaining() {
            if let Some(tx) = next_e_tx(&mut data)? {
                pfbs.push(tx)
            }
        }
        // assert_eq!(data_from_node, data);
    }
    Ok(pfbs)
}

fn next_e_tx(
    mut data: &mut std::io::Cursor<Vec<u8>>,
) -> Result<Option<MsgPayForData>, Box<dyn std::error::Error>> {
    let len = decode_varint(&mut data).expect("Varint must be valid");
    dbg!("Found tx with", len);
    let backup = data.position();
    let tx = match MalleatedTx::decode(&mut data) {
        Ok(malleated) => {
            // The hash length must be 32
            if malleated.original_tx_hash.len() != 32 {
                data.set_position(backup);
                TxType::Other(Tx::decode(data)?)
            } else {
                TxType::Pfd(malleated)
            }
        }
        Err(_) => {
            data.set_position(backup);
            TxType::Other(Tx::decode(data)?)
        }
    };

    let sdk_tx = match tx {
        TxType::Pfd(malleated) => {
            let inner = malleated.tx.clone();
            Tx::decode(inner)?
        }
        TxType::Other(tx) => tx,
    };

    let body = sdk_tx.body.expect("transaction must have body");
    if body.messages.len() == 1 {
        for msg in body.messages {
            if msg.type_url == "/payment.MsgPayForData" {
                let pfd = MsgPayForData::decode(std::io::Cursor::new(msg.value))?;
                return Ok(Some(pfd));
            }
        }
    }
    Ok(None)
}

// pub fn get_namespace_data(
//     height: u64,
//     namespace: [u8; 8],
// ) -> Result<NamespaceGroup, Box<dyn std::error::Error>> {
//     let rpc_addr = format!(
//         "http://localhost:26659/namespaced_shares/{}/height/{}",
//         hex::encode(namespace),
//         height
//     );

//     let body = reqwest::blocking::get(rpc_addr)?.text()?;
//     let response: NamespacedSharesResponse = serde_json::from_str(&body)?;
//     let shares = NamespaceGroup::from_b64_shares(&response.shares)?;
//     Ok(shares)
// }

// pub fn get_header(height: u64) -> Result<tendermint::block::Header, Box<dyn std::error::Error>> {
//     let rpc_addr = format!("http://localhost:26659/header/{}", height);

//     let body = reqwest::blocking::get(rpc_addr)?.text()?;
//     let response: CelestiaHeaderResponse = serde_json::from_str(&body)?;
//     // let shares = NamespaceGroup::from_b64_shares(&response.shares)?;
//     Ok(response.header)
// }

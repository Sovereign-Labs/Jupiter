use std::ops::Not;

use serde::{Deserialize, Serialize};
use sovereign_sdk::core::traits::{Address, Blockheader, CanonicalHash};

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

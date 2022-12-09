use std::collections::HashMap;

use sovereign_sdk::{
    core::traits::Address,
    da::{self, TxWithSender},
    Bytes,
};

use crate::{
    da_service::FilteredCelestiaBlock,
    share_commit::recreate_commitment,
    shares::{Blob, BlobIterator, BlobRefIterator},
    BlobWithSender, CelestiaHeader, Sha2Hash, H160,
};
use hex_literal::hex;

pub struct Celestia {
    pub db: HashMap<tendermint::Hash, FilteredCelestiaBlock>,
}

impl TxWithSender<CelestiaAddress> for BlobWithSender {
    type Data = BlobIterator;
    fn sender(&self) -> CelestiaAddress {
        self.sender.clone()
    }

    fn data(&self) -> Self::Data {
        self.blob.clone().into_iter()
    }
}

#[derive(Debug, PartialEq, Clone)]
pub struct CelestiaAddress(Vec<u8>);

impl AsRef<[u8]> for CelestiaAddress {
    fn as_ref(&self) -> &[u8] {
        self.0.as_ref()
    }
}
impl Address for CelestiaAddress {}

impl<'a> TryFrom<&'a [u8]> for CelestiaAddress {
    type Error = ();

    fn try_from(value: &'a [u8]) -> Result<Self, ()> {
        Ok(Self(value.to_vec()))
    }
}

impl da::DaApp for Celestia {
    type Blockhash = tendermint::Hash;

    type Address = CelestiaAddress;

    type Header = CelestiaHeader;

    type BlobTransaction = BlobWithSender;

    type InclusionProof = ();

    type InclusionMultiProof = ();

    type CompletenessProof = ();

    type Error = ();

    const ADDRESS_LENGTH: usize = 20;

    const RELATIVE_GENESIS: Self::Blockhash = tendermint::Hash::Sha256(hex!(
        "7D99C8487B0914AA6851549CD59440FAFC20697B9029DF7AD07A681A50ACA747"
    ));

    fn get_relevant_txs(&self, blockhash: &Self::Blockhash) -> Vec<Self::BlobTransaction> {
        let filtered_block = self
            .db
            .get(blockhash)
            .expect("Must only call get txs on extant blocks");

        let mut output = Vec::new();
        for blob in filtered_block.rollup_data.blobs() {
            let commitment = recreate_commitment(4, blob.clone()).expect("blob must be valid");
            let sender = filtered_block
                .relevant_txs
                .get(&commitment[..])
                .expect("blob must be relevant")
                .signer
                .clone();

            let blob_tx = BlobWithSender {
                blob: blob.into(),
                sender: CelestiaAddress(sender.as_bytes().to_vec()),
            };
            output.push(blob_tx)
        }
        output
    }

    fn get_relevant_txs_with_proof(
        &self,
        blockhash: &Self::Blockhash,
    ) -> (
        Vec<Self::BlobTransaction>,
        Self::InclusionMultiProof,
        Self::CompletenessProof,
    ) {
        todo!()
    }

    fn verify_relevant_tx_list(
        &self,
        blockheader: &Self::Header,
        txs: &Vec<Self::BlobTransaction>,
        inclusion_proof: &Self::InclusionMultiProof,
        completeness_proof: &Self::CompletenessProof,
    ) -> Result<(), Self::Error> {
        todo!()
    }
}

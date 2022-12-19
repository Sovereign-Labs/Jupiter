use std::collections::HashMap;

use nmt_rs::{db::MemDb, NamespaceMerkleTree};
use sovereign_sdk::{
    core::traits::Address,
    da::{self, TxWithSender},
    Bytes,
};
use tendermint::merkle;

use crate::{
    da_service::{FilteredCelestiaBlock, ValidationError, ROLLUP_NAMESPACE},
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

    type CompletenessProof = Vec<RelevantRowProof>;

    type Error = ValidationError;

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
            println!("Successfully recreated commitment");
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
        let relevant_txs = self.get_relevant_txs(blockhash);

        let filtered_block = self
            .db
            .get(blockhash)
            .expect("Must only call get txs on extant blocks");

        let mut relevant_rows = filtered_block.relevant_rows.iter();

        let mut row_proofs = Vec::new();
        for row_root in filtered_block.header.dah.row_roots.iter() {
            if row_root.contains(ROLLUP_NAMESPACE) {
                let mut nmt = NamespaceMerkleTree::<MemDb>::new();
                let next_row = relevant_rows
                    .next()
                    .expect("All relevant rows must be present");
                assert!(row_root == &next_row.root);
                for share in &next_row.row {
                    nmt.push_leaf(share.as_ref(), ROLLUP_NAMESPACE)
                        .expect("shares are pushed in order");
                }
                assert_eq!(&nmt.root(), row_root);
                let (leaves, proof) = nmt.get_namespace_with_proof(ROLLUP_NAMESPACE);
                let row_proof = RelevantRowProof { leaves, proof };
                row_proofs.push(row_proof)
            }
        }

        assert!(filtered_block.header.validate_dah().is_ok());

        (relevant_txs, (), row_proofs)
    }

    fn verify_relevant_tx_list(
        &self,
        blockheader: &Self::Header,
        txs: &Vec<Self::BlobTransaction>,
        inclusion_proof: &Self::InclusionMultiProof,
        completeness_proof: &Self::CompletenessProof,
    ) -> Result<(), Self::Error> {
        // Check the completeness of the provided blob txs
        blockheader.validate_dah()?;
        let mut relevant_row_proofs = completeness_proof.iter();
        for row_root in blockheader.dah.row_roots.iter() {
            if row_root.contains(ROLLUP_NAMESPACE) {
                let row_proof = relevant_row_proofs
                    .next()
                    .expect("All proofs must be present");
                row_proof
                    .proof
                    .clone()
                    .verify(row_root, &row_proof.leaves, ROLLUP_NAMESPACE)
                    .expect("Proofs must be valid");
            }
        }

        // Check the correctness of the sender field
        // TODO(@preston-evans98): Remove this logic if Celestia adds blob.sender metadata directly into blob
        todo!()
    }
}

#[derive(Debug, PartialEq, Clone)]

pub struct RelevantRowProof {
    leaves: Vec<Vec<u8>>,
    proof: nmt_rs::Proof,
}

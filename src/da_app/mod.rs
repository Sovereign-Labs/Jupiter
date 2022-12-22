use std::collections::HashMap;

use nmt_rs::{db::MemDb, NamespaceId, NamespaceMerkleTree};
use prost::Message;
use sovereign_sdk::{
    da::{self, TxWithSender},
    Bytes,
};

pub mod address;
mod proofs;

use crate::{
    da_service::{
        FilteredCelestiaBlock, ValidationError, PARITY_SHARES_NAMESPACE, ROLLUP_NAMESPACE,
        TRANSACTIONS_NAMESPACE,
    },
    payment::MsgPayForData,
    share_commit::recreate_commitment,
    shares::{read_varint, BlobIterator, BlobRef, NamespaceGroup, Share},
    BlobWithSender, CelestiaHeader, MalleatedTx, Tx,
};
use hex_literal::hex;
use proofs::*;

use self::address::CelestiaAddress;

pub struct CelestiaApp {
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

impl da::DaApp for CelestiaApp {
    type Blockhash = tendermint::Hash;

    type Address = CelestiaAddress;

    type Header = CelestiaHeader;

    type BlobTransaction = BlobWithSender;

    type InclusionMultiProof = Vec<BlobProof>;

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
            let commitment = recreate_commitment(filtered_block.square_size(), blob.clone())
                .expect("blob must be valid");
            println!("Successfully recreated commitment");
            let sender = filtered_block
                .relevant_txs
                .get(&commitment[..])
                .expect("blob must be relevant")
                .0
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
        let square_size = filtered_block.square_size();

        let mut needed_tx_shares = Vec::new();

        for tx in relevant_txs.iter() {
            let commitment = recreate_commitment(square_size, BlobRef::with(&tx.blob.0))
                .expect("commitment is valid");

            let (_, position) = filtered_block
                .relevant_txs
                .get(&commitment[..])
                .expect("commitment must exist in map");
            needed_tx_shares.push(position.clone());
        }

        let mut needed_tx_shares = needed_tx_shares.into_iter().peekable();

        // Compute the completeness proof for the namespace data by computing the merkle tree for
        // each relevant row, and the generating a namespace proof from that tree
        let mut rollup_row_proofs = Vec::new();

        let mut current_tx_proof: BlobProof = BlobProof { proof: Vec::new() };
        let mut tx_proofs: Vec<BlobProof> = Vec::with_capacity(needed_tx_shares.len());
        for (row_idx, row_root) in filtered_block.header.dah.row_roots.iter().enumerate() {
            if row_root.contains(TRANSACTIONS_NAMESPACE) || row_root.contains(ROLLUP_NAMESPACE) {
                let mut nmt = NamespaceMerkleTree::<MemDb>::new();
                let next_row = relevant_rows
                    .next()
                    .expect("All relevant rows must be present");
                assert!(row_root == &next_row.root);
                for (idx, share) in next_row.row.iter().enumerate() {
                    // Shares in the two left-hand quadrants are prefixed with their namespace, while parity
                    // shares (in the right-hand) quadrants always have the PARITY_SHARES_NAMESPACE
                    let namespace = if idx < next_row.row.len() / 2 {
                        share.namespace()
                    } else {
                        PARITY_SHARES_NAMESPACE
                    };
                    nmt.push_leaf(share.as_serialized(), namespace)
                        .expect("shares are pushed in order");
                }
                assert_eq!(&nmt.root(), row_root);
                while let Some(next_needed_share) = needed_tx_shares.peek_mut() {
                    // If the next needed share falls in this row
                    let row_adjustment = square_size * row_idx;
                    let start_column_number = next_needed_share.share_range.start - row_adjustment;
                    if start_column_number < square_size {
                        let end_column_number = next_needed_share.share_range.end - row_adjustment;
                        if end_column_number <= square_size {
                            let (shares, proof) =
                                nmt.get_range_with_proof(start_column_number..end_column_number);

                            current_tx_proof.proof.push(EtxRangeProof {
                                shares,
                                proof,
                                start_offset: next_needed_share.start_offset,
                                start_share_idx: next_needed_share.share_range.start,
                            });
                            tx_proofs.push(current_tx_proof);
                            current_tx_proof = BlobProof { proof: Vec::new() };
                            let _ = needed_tx_shares.next();
                        } else {
                            let (shares, proof) =
                                nmt.get_range_with_proof(start_column_number..square_size);

                            current_tx_proof.proof.push(EtxRangeProof {
                                shares,
                                proof,
                                start_offset: next_needed_share.start_offset,
                                start_share_idx: next_needed_share.share_range.start,
                            });
                            next_needed_share.share_range.start = square_size * (row_idx + 1);
                            next_needed_share.start_offset = 0;

                            break;
                        }
                    } else {
                        break;
                    }
                }

                if row_root.contains(ROLLUP_NAMESPACE) {
                    let (leaves, proof) = nmt.get_namespace_with_proof(ROLLUP_NAMESPACE);
                    let row_proof = RelevantRowProof { leaves, proof };
                    rollup_row_proofs.push(row_proof)
                }
            }
        }

        assert!(filtered_block.header.validate_dah().is_ok());

        // We use the abuse the inclusion proof for each blob-with-sender to link the
        // sender with the blob data. Future changes to Celestia should make this unnecessary.
        // filtered_block

        (relevant_txs, tx_proofs, rollup_row_proofs)
    }

    // Workflow:
    // 1. Validate DAH
    // 2. For row in dah, if it might contain any relevant transactions, check the "Completeness proof"
    // For the data thus generated, deserialize into blobs. Compute the share commitments
    // For each blob, find the relevant "inclusion_proof". Verify the inclusion of the shares, then
    // deserialize to find the commitment and sender. Verify that the share_commitment matches the blob,
    // and the sender matches the blob sender. Done
    fn verify_relevant_tx_list(
        &self,
        blockheader: &Self::Header,
        txs: &Vec<Self::BlobTransaction>,
        tx_proofs: Self::InclusionMultiProof,
        row_proofs: Self::CompletenessProof,
    ) -> Result<(), Self::Error> {
        // Check the completeness of the provided blob txs
        blockheader.validate_dah()?;
        let mut relevant_row_proofs = row_proofs.into_iter();
        let mut tx_iter = txs.iter();
        // Verify namespace completeness
        let square_size = blockheader.dah.row_roots.len();

        // Check the validity and completeness of the rollup share proofs
        let mut rollup_shares_u8: Vec<Vec<u8>> = Vec::new();
        for row_root in blockheader.dah.row_roots.iter() {
            if row_root.contains(ROLLUP_NAMESPACE) {
                let row_proof = relevant_row_proofs
                    .next()
                    .expect("All proofs must be present");
                row_proof
                    .proof
                    .verify_complete_namespace(row_root, &row_proof.leaves, ROLLUP_NAMESPACE)
                    .expect("Proofs must be valid");

                for leaf in row_proof.leaves {
                    rollup_shares_u8.push(leaf)
                }
            }
        }
        if rollup_shares_u8.is_empty() {
            if txs.is_empty() {
                return Ok(());
            }
            return Err(ValidationError::MissingTx);
        }
        let namespace = NamespaceGroup::from_shares_unchecked(rollup_shares_u8);

        // Check the e-tx proofs...
        // TODO(@preston-evans98): Remove this logic if Celestia adds blob.sender metadata directly into blob
        for (blob, tx_proof) in namespace.blobs().zip(tx_proofs.into_iter()) {
            // Force the row number to be monotonically increasing
            let start_offset = tx_proof.proof[0].start_offset;

            // Verify each sub-proof and flatten the shares back into a sequential array
            // First, enforce that the sub-proofs cover a contiguous range of shares
            for [l, r] in tx_proof.proof.array_windows::<2>() {
                assert_eq!(l.start_share_idx + l.shares.len(), r.start_share_idx)
            }
            let mut tx_shares = Vec::new();
            // Then, verify the sub proofs
            for sub_proof in tx_proof.proof.into_iter() {
                let row_num = sub_proof.start_share_idx / square_size;
                let root = &blockheader.dah.row_roots[row_num];
                sub_proof
                    .proof
                    .verify_range(root, &sub_proof.shares, TRANSACTIONS_NAMESPACE)
                    .map_err(|_| ValidationError::InvalidEtxProof)?;
                tx_shares.extend(
                    sub_proof
                        .shares
                        .into_iter()
                        .map(|share_vec| Share::new(share_vec.into())),
                )
            }

            // Next, ensure that the start_index is valid
            if !tx_shares[0].is_valid_tx_start(start_offset) {
                return Err(ValidationError::InvalidEtxProof);
            }
            let trailing_shares = tx_shares[1..]
                .iter()
                .map(|share| share.data_ref().iter())
                .flatten();
            let tx_data: Vec<u8> = tx_shares[0].data_ref()[start_offset..]
                .iter()
                .chain(trailing_shares)
                .map(|x| *x)
                .collect();

            let (len, len_of_len) = {
                let cursor = std::io::Cursor::new(&tx_data);
                read_varint(cursor).expect("tx must be length prefixed")
            };
            let cursor = std::io::Cursor::new(&tx_data[len_of_len..len as usize + len_of_len]);

            let malleated =
                MalleatedTx::decode(cursor).map_err(|_| ValidationError::InvalidEtxProof)?;
            if malleated.original_tx_hash.len() != 32 {
                return Err(ValidationError::InvalidEtxProof);
            }
            let sdk_tx = Tx::decode(malleated.tx).map_err(|_| ValidationError::InvalidEtxProof)?;
            let body = sdk_tx.body.ok_or(ValidationError::InvalidEtxProof)?;
            for msg in body.messages {
                if msg.type_url == "/payment.MsgPayForData" {
                    let pfd = MsgPayForData::decode(std::io::Cursor::new(msg.value))
                        .map_err(|_| ValidationError::InvalidEtxProof)?;
                    let tx = tx_iter.next().ok_or(ValidationError::MissingTx)?;
                    if tx.sender.as_ref() != pfd.signer.as_bytes() {
                        return Err(ValidationError::InvalidSigner);
                    }

                    let blob_data: Bytes = blob.data().collect();
                    let tx_data: Bytes = tx.data().collect();
                    assert_eq!(blob_data, tx_data)
                    // TODO - link share commitment to blob commitment
                }
            }
        }

        // Check the correctness of the sender field

        // todo!()
        Ok(())
    }
}

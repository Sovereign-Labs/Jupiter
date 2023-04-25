use borsh::{BorshDeserialize, BorshSerialize};
use nmt_rs::{NamespaceProof, NamespacedSha2Hasher};

use crate::{
    da_service::{FilteredCelestiaBlock, ROLLUP_NAMESPACE},
    share_commit::recreate_commitment,
    shares::BlobRef,
    BlobWithSender,
};

#[derive(Debug, PartialEq, Clone, BorshDeserialize, BorshSerialize)]
pub struct EtxProof {
    pub proof: Vec<EtxRangeProof>,
}

#[derive(Debug, PartialEq, Clone, BorshDeserialize, BorshSerialize)]
pub struct EtxRangeProof {
    pub shares: Vec<Vec<u8>>,
    pub proof: NamespaceProof<NamespacedSha2Hasher>,
    pub start_share_idx: usize,
    pub start_offset: usize,
}

#[derive(Debug, PartialEq, Clone, BorshDeserialize, BorshSerialize)]

pub struct RelevantRowProof {
    pub leaves: Vec<Vec<u8>>,
    pub proof: NamespaceProof<NamespacedSha2Hasher>,
}

#[derive(Debug, PartialEq, Clone)]
pub struct CompletenessProof(pub Vec<RelevantRowProof>);

impl CompletenessProof {
    pub fn from_filtered_block(block: &FilteredCelestiaBlock) -> Self {
        let mut row_proofs = Vec::new();
        for row in block.rollup_rows.iter() {
            let mut nmt = row.merklized();
            let (leaves, proof) = nmt.get_namespace_with_proof(ROLLUP_NAMESPACE);
            let row_proof = RelevantRowProof { leaves, proof };
            row_proofs.push(row_proof)
        }
        Self(row_proofs)
    }
}

pub struct CorrectnessProof(pub Vec<EtxProof>);

impl CorrectnessProof {
    pub fn for_block(block: &FilteredCelestiaBlock, relevant_txs: &Vec<BlobWithSender>) -> Self {
        let mut needed_tx_shares = Vec::new();

        // Extract (and clone) the position of each transaction
        for tx in relevant_txs.iter() {
            let commitment = recreate_commitment(block.square_size(), BlobRef::with(&tx.blob.0))
                .expect("commitment is valid");

            let (_, position) = block
                .relevant_pfbs
                .get(&commitment[..])
                .expect("commitment must exist in map");
            needed_tx_shares.push(position.clone());
        }

        let mut needed_tx_shares = needed_tx_shares.into_iter().peekable();
        let mut current_tx_proof: EtxProof = EtxProof { proof: Vec::new() };
        let mut tx_proofs: Vec<EtxProof> = Vec::with_capacity(relevant_txs.len());

        for (row_idx, row) in block.pfb_rows.iter().enumerate() {
            let mut nmt = row.merklized();
            while let Some(next_needed_share) = needed_tx_shares.peek_mut() {
                // If the next needed share falls in this row
                let row_start_idx = block.square_size() * row_idx;
                let start_column_number = next_needed_share.share_range.start - row_start_idx;
                if start_column_number < block.square_size() {
                    let end_column_number = next_needed_share.share_range.end - row_start_idx;
                    if end_column_number <= block.square_size() {
                        let (shares, proof) =
                            nmt.get_range_with_proof(start_column_number..end_column_number);

                        current_tx_proof.proof.push(EtxRangeProof {
                            shares,
                            proof,
                            start_offset: next_needed_share.start_offset,
                            start_share_idx: next_needed_share.share_range.start,
                        });
                        tx_proofs.push(current_tx_proof);
                        current_tx_proof = EtxProof { proof: Vec::new() };
                        let _ = needed_tx_shares.next();
                    } else {
                        let (shares, proof) =
                            nmt.get_range_with_proof(start_column_number..block.square_size());

                        current_tx_proof.proof.push(EtxRangeProof {
                            shares,
                            proof,
                            start_offset: next_needed_share.start_offset,
                            start_share_idx: next_needed_share.share_range.start,
                        });
                        next_needed_share.share_range.start = block.square_size() * (row_idx + 1);
                        next_needed_share.start_offset = 0;

                        break;
                    }
                } else {
                    break;
                }
            }
        }
        Self(tx_proofs)
    }
}

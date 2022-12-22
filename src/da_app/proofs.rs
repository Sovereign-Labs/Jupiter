use std::ops::Range;

use nmt_rs::{db::MemDb, NamespaceMerkleTree};

use crate::{
    da_service::{
        FilteredCelestiaBlock, Row, PARITY_SHARES_NAMESPACE, ROLLUP_NAMESPACE,
        TRANSACTIONS_NAMESPACE,
    },
    share_commit::recreate_commitment,
    shares::BlobRef,
    BlobWithSender, DataAvailabilityHeader,
};

#[derive(Debug, PartialEq, Clone)]
pub struct EtxProof {
    pub proof: Vec<EtxRangeProof>,
}

#[derive(Debug, PartialEq, Clone)]
pub struct EtxRangeProof {
    pub shares: Vec<Vec<u8>>,
    pub proof: nmt_rs::Proof,
    pub start_share_idx: usize,
    pub start_offset: usize,
}

#[derive(Debug, PartialEq, Clone)]

pub struct RelevantRowProof {
    pub leaves: Vec<Vec<u8>>,
    pub proof: nmt_rs::Proof,
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

// fn split_at_row_boundary(square_size: usize, range: Range<usize>) -> Vec<Range<usize>> {
//     let mut output = Vec::new();
//     loop {
//         let start_of_next_row = (range.start / square_size) + square_size;
//         if range.end <= start_of_next_row {
//             output.push(range.start..range.end);
//             return output;
//         } else {
//             output.push(range.start..start_of_next_row);
//             range.start = start_of_next_row;
//         }
//     }
// }

impl CorrectnessProof {
    pub fn for_block(block: &FilteredCelestiaBlock, relevant_txs: &Vec<BlobWithSender>) -> Self {
        let mut needed_tx_shares = Vec::new();

        for tx in relevant_txs.iter() {
            let commitment = recreate_commitment(block.square_size(), BlobRef::with(&tx.blob.0))
                .expect("commitment is valid");

            let (_, position) = block
                .relevant_etxs
                .get(&commitment[..])
                .expect("commitment must exist in map");
            needed_tx_shares.push(position.clone());
        }

        let mut needed_tx_shares = needed_tx_shares.into_iter().peekable();
        let mut current_tx_proof: EtxProof = EtxProof { proof: Vec::new() };
        let mut tx_proofs: Vec<EtxProof> = Vec::with_capacity(relevant_txs.len());

        for (row_idx, row) in block.etx_rows.iter().enumerate() {
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

// // if relevant_txs.len() == 0 {
// //     return;
// // }

// let relevant_txs = relevant_txs.iter().map(|tx|
// {
//     let tx_commitment =
//             recreate_commitment(block.square_size(), BlobRef::with(&tx.blob.0))
//                 .expect("commitment is valid");
//     let (_, tx_position) = block
//         .relevant_etxs
//         .get(&tx_commitment[..])
//         .expect("commitment must exist in map")
//         .clone();

//     split_at_row_boundary(block.square_size(), tx_position.share_range);

// });

// // let mut relevant_txs = relevant_txs.iter();
// // let mut current_tx_proof = EtxProof { proof: vec![] };
// // let tx_proofs = Vec::new();

// // for (row_num, row) in block.etx_rows.iter().enumerate()
// // // .map(|(row_num, row)| ((row_num + 1) * block.square_size(), row))
// // {
// //     let mut nmt = row.merklized();
// //     let row_adjustment = row_num * block.square_size();
//     while let Some(tx) = relevant_txs.next() {
//         let tx_commitment =
//             recreate_commitment(block.square_size(), BlobRef::with(&tx.blob.0))
//                 .expect("commitment is valid");
//         let (_, tx_position) = block
//             .relevant_etxs
//             .get(&tx_commitment[..])
//             .expect("commitment must exist in map")
//             .clone();

// //         let next_row_start_idx = (row_num + 1) * block.square_size();

// //         let (shares, proof) = nmt.get_range_with_proof(
// //             tx_position.share_range.start - row_adjustment
// //                 ..tx_position.share_range.end - row_adjustment,
// //         );
// //         current_tx_proof.proof.push(EtxRangeProof {
// //             shares,
// //             proof,
// //             start_share_idx: tx_position.share_range.start,
// //             start_offset: tx_position.start_offset,
// //         });
// //         if tx_position.share_range.end <= next_row_start_idx {
// //             tx_proofs.push(current_tx_proof)
// //         }

// //         // relevant_txs.nth_back(1)
// //     }
// // }

// // let row_data_iter = block.etx_rows.iter();
// // let nmt = row_data_iter.next().expect("row must exist").merklized()
// // for tx in relevant_txs {
// //     let tx_commitment = recreate_commitment(block.square_size(), BlobRef::with(&tx.blob.0))
// //         .expect("commitment is valid");
// //     let (_, position) = block
// //         .relevant_etxs
// //         .get(&tx_commitment[..])
// //         .expect("commitment must exist in map")
// //         .clone();

// //     // A single e-tx might be split across multiple columns
// //     // let row_data = row_data_iter.next().expect("msg");
// //     let start_idx = position.share_range.start;
// //     let end_idx = position.share_range.end - 1;

// //     while start_idx < end_idx {
// //         let current_row_num = block.get_row_number(start_idx);
// //         let last_idx_to_prove =
// //             std::cmp::min(current_row_num * block.square_size(), end_idx);

// //         let start_column = block.get_col_number(start_idx);
// //         let end_column = block.get_col_number(last_idx_to_prove);

// //         nmt.get_range_with_proof(start_column..end_column + 1);

// //         start_idx = last_idx_to_prove + 1;
// //     }
// // }
// // let mut needed_tx_shares = Vec::new();
// // for tx in relevant_txs.iter() {
// //     let commitment = recreate_commitment(block.square_size(), BlobRef::with(&tx.blob.0))
// //         .expect("commitment is valid");

// //     let (_, position) = block
// //         .relevant_etxs
// //         .get(&commitment[..])
// //         .expect("commitment must exist in map");
// //     needed_tx_shares.push(position.clone());
// // }

// // // let needed_tx_shares = needed_tx_shares.into_iter().peekable();
// // while let Some(next_needed_share) = needed_tx_shares.peek_mut() {
// //     // If the next needed share falls in this row
// //     let row_adjustment = square_size * row_idx;
// //     let start_column_number = next_needed_share.share_range.start - row_adjustment;
// //     if start_column_number < square_size {
// //         let end_column_number = next_needed_share.share_range.end - row_adjustment;
// //         if end_column_number <= square_size {
// //             let (shares, proof) =
// //                 nmt.get_range_with_proof(start_column_number..end_column_number);

// //             current_tx_proof.proof.push(EtxRangeProof {
// //                 shares,
// //                 proof,
// //                 start_offset: next_needed_share.start_offset,
// //                 start_share_idx: next_needed_share.share_range.start,
// //             });
// //             tx_proofs.push(current_tx_proof);
// //             current_tx_proof = EtxProof { proof: Vec::new() };
// //             let _ = needed_tx_shares.next();
// //         } else {
// //             let (shares, proof) =
// //                 nmt.get_range_with_proof(start_column_number..square_size);

// //             current_tx_proof.proof.push(EtxRangeProof {
// //                 shares,
// //                 proof,
// //                 start_offset: next_needed_share.start_offset,
// //                 start_share_idx: next_needed_share.share_range.start,
// //             });
// //             next_needed_share.share_range.start = square_size * (row_idx + 1);
// //             next_needed_share.start_offset = 0;

// //             break;
// //         }
// //     } else {
// //         break;
// //     }
// // }
// {}

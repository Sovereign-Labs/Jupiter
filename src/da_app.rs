// use sovereign_sdk::da;

// use crate::{CelestiaHeader, Sha2Hash, H160};
// use hex_literal::hex;

// pub struct Celestia;

// impl da::DaApp for Celestia {
//     type Blockhash = Sha2Hash;

//     type Address = H160;

//     type Header = CelestiaHeader;

//     type Transaction;

//     type InclusionProof;

//     type InclusionMultiProof;

//     type CompletenessProof = ();

//     type Error;

//     const ADDRESS_LENGTH: usize = 20;

//     const RELATIVE_GENESIS: Self::Blockhash = Sha2Hash(hex!(
//         "7D99C8487B0914AA6851549CD59440FAFC20697B9029DF7AD07A681A50ACA747"
//     ));

//     fn get_relevant_txs(&self, blockhash: &Self::Blockhash) -> Vec<Self::Transaction> {
//         todo!()
//     }

//     fn get_relevant_txs_with_proof(
//         &self,
//         blockhash: &Self::Blockhash,
//     ) -> (
//         Vec<Self::Transaction>,
//         Self::InclusionMultiProof,
//         Self::CompletenessProof,
//     ) {
//         todo!()
//     }

//     fn verify_relevant_tx_list(
//         &self,
//         blockheader: &Self::Header,
//         txs: &Vec<Self::Transaction>,
//         inclusion_proof: &Self::InclusionMultiProof,
//         completeness_proof: &Self::CompletenessProof,
//     ) -> Result<(), Self::Error> {
//         todo!()
//     }
// }

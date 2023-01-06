use std::collections::HashMap;

use jupiter::{da_app::CelestiaApp, da_service::CelestiaService};
use sovereign_sdk::{core::traits::CanonicalHash, da::DaApp, services::da::DaService};

// const ROLLUP_NAMESPACE: [u8; 8] = *b"sov-test";

// I sent the following test blob in block 80873. Namespace: b'sov-test'
// b'{"key": "testkey", "value": "testvalue"}'

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let current_height = 80873;
    let mut db = HashMap::new();
    let mut ordered_hashes = Vec::new();
    let mut ordered_headers = Vec::new();
    for i in 0..1 {
        let block = CelestiaService::get_finalized_at(current_height + i).await?;
        let hash = block.header.hash();
        println!("Block hash: {:?}", &hash);
        ordered_hashes.push(hash.clone());
        ordered_headers.push(block.header.clone());
        db.insert(hash, block);
    }

    let celestia = CelestiaApp { db };
    for (hash, header) in ordered_hashes.into_iter().zip(ordered_headers) {
        let (txs, inclusion_proof, completeness_proof) =
            celestia.get_relevant_txs_with_proof(&hash);
        celestia
            .verify_relevant_tx_list(&header, &txs, inclusion_proof, completeness_proof)
            .expect("Validation should succeed");
    }

    Ok(())
}

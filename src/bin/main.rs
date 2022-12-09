use std::collections::HashMap;

use jupiter::{da_app::Celestia, da_service::CelestiaService};
use sovereign_sdk::da::{DaApp, DaService};

// const ROLLUP_NAMESPACE: [u8; 8] = *b"sov-test";

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let current_height = 45963;
    let mut db = HashMap::new();
    let mut ordered_hashes = Vec::new();
    for i in 0..1 {
        let block = CelestiaService::get_finalized_at(current_height + i).await?;
        let hash = block.header.hash();
        ordered_hashes.push(hash.clone());
        db.insert(hash, block);
    }

    let celestia = Celestia { db };
    for hash in ordered_hashes {
        celestia.get_relevant_txs(&hash);
    }

    Ok(())
}

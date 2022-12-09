use std::collections::HashMap;

use sovereign_node::{da_app::Celestia, da_service::CelestiaService};
use sovereign_sdk::da::DaService;

// const ROLLUP_NAMESPACE: [u8; 8] = *b"sov-test";

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut current_height = 45963;
    let mut db = HashMap::new();
    for i in 0..3 {
        let block = CelestiaService::get_finalized_at(current_height + i).await?;
        let hash = block.header.hash();
        db.insert(hash, block);
    }

    let celestia = Celestia { db };

    Ok(())
}

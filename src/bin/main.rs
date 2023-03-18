use std::collections::HashMap;

use jsonrpsee::http_client::HeaderMap;
use jupiter::{da_app::CelestiaApp, da_service::CelestiaService};
use sovereign_sdk::{core::traits::CanonicalHash, da::DaLayerTrait, services::da::DaService};
use tracing::Level;

// const ROLLUP_NAMESPACE: [u8; 8] = *b"sov-test";

// I sent the following test blob in block 80873. Namespace: b'sov-test'
// b'{"key": "testkey", "value": "testvalue"}'

const NODE_AUTH_TOKEN: &'static str = "";
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let subscriber = tracing_subscriber::fmt()
        .with_max_level(Level::DEBUG)
        .finish();
    tracing::subscriber::set_global_default(subscriber)
        .map_err(|_err| eprintln!("Unable to set global default subscriber"))
        .expect("Cannot fail to set subscriber");

    // trace!("starting program!");
    let mut headers = HeaderMap::new();
    headers.insert(
        "Authorization",
        format!("Bearer {}", NODE_AUTH_TOKEN).parse().unwrap(),
    );
    let client = jsonrpsee::http_client::HttpClientBuilder::default()
        .set_headers(headers)
        .build("http://localhost:11111/")
        .unwrap();

    // info!("getting block!");
    let service = CelestiaService::with_client(client);
    // let res = service.get_finalized_at(80873).await?;
    let res = service.get_finalized_at(24577).await?;
    dbg!(res);

    let current_height = 24577;
    let mut db = HashMap::new();
    let mut ordered_hashes = Vec::new();
    let mut ordered_headers = Vec::new();
    for i in 0..1 {
        let block = service.get_finalized_at(current_height + i).await?;
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
    println!("Successfully vereified relevant tx lists!");

    Ok(())
}

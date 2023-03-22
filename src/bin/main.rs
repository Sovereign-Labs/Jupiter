use std::collections::HashMap;

use jsonrpsee::http_client::HeaderMap;
use jupiter::{da_app::CelestiaApp, da_service::CelestiaService};
use sovereign_sdk::{
    core::traits::CanonicalHash,
    da::{BlobTransactionTrait, DaLayerTrait},
    services::da::DaService,
};
use tracing::Level;

// const ROLLUP_NAMESPACE: [u8; 8] = *b"sov-test";

// I sent the following test blob in block 275345. on arabica-6 Namespace: b'sov-test'
// b'{"key": "testkey", "value": "testvalue"}'

const NODE_AUTH_TOKEN: &'static str = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJBbGxvdyI6WyJwdWJsaWMiLCJyZWFkIiwid3JpdGUiLCJhZG1pbiJdfQ.nHzh7kWvC3puCYgcMJRuNlMudwf6xGagETNdQyRQQ_s";
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let subscriber = tracing_subscriber::fmt()
        .with_max_level(Level::WARN)
        .finish();
    tracing::subscriber::set_global_default(subscriber)
        .map_err(|_err| eprintln!("Unable to set global default subscriber"))
        .expect("Cannot fail to set subscriber");

    let mut headers = HeaderMap::new();
    headers.insert(
        "Authorization",
        format!("Bearer {}", NODE_AUTH_TOKEN).parse().unwrap(),
    );
    let client = jsonrpsee::http_client::HttpClientBuilder::default()
        .set_headers(headers)
        .build("http://localhost:11111/")
        .unwrap();

    let service = CelestiaService::with_client(client);

    // Verify a known block
    let current_height = 275345;
    let mut db = HashMap::new();
    let mut ordered_hashes = Vec::new();
    let mut ordered_headers = Vec::new();
    for i in 0..1 {
        let block = service.get_finalized_at(current_height + i).await?;
        let hash = block.header.hash();
        ordered_hashes.push(hash.clone());
        ordered_headers.push(block.header.clone());
        db.insert(hash, block);
    }

    let celestia = CelestiaApp { db };
    for (hash, header) in ordered_hashes.into_iter().zip(ordered_headers) {
        let (txs, inclusion_proof, completeness_proof) =
            celestia.get_relevant_txs_with_proof(&hash);
        let verification_result =
            celestia.verify_relevant_tx_list(&header, &txs, inclusion_proof, completeness_proof);
        verification_result.expect("verification must succeeds");
        for tx in txs {
            let raw_data: Vec<u8> = tx.data().collect();
            assert!(&raw_data == r#"{"key": "testkey", "value": "testvalue"}"#.as_bytes())
        }
    }
    println!("Successfully verified relevant tx lists!");

    Ok(())
}

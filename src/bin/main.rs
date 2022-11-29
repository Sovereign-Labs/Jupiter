use sovereign_node::CelestiaHeaderResponse;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let rpc_addr = "http://localhost:26659/header/45000";
    // http://localhost:26659/namespaced_data/0000000000000001/height/45963

    let body = reqwest::get(rpc_addr).await?.text().await?;

    let response: CelestiaHeaderResponse = serde_json::from_str(&body)?;
    dbg!(response);

    Ok(())
}

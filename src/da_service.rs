use std::{collections::HashMap, future::Future, pin::Pin};

use sovereign_sdk::{da::DaService, Bytes};

const ROLLUP_NAMESPACE: [u8; 8] = hex_literal::hex!("db841b1c364eb119");

use crate::{
    parse_tx_namespace, payment::MsgPayForData, shares::NamespaceGroup, CelestiaHeader,
    CelestiaHeaderResponse, NamespacedSharesResponse,
};

#[derive(Debug, Clone)]
pub struct CelestiaService;

#[derive(Debug, Clone, PartialEq)]
pub struct FilteredCelestiaBlock {
    pub header: tendermint::block::Header,
    pub rollup_data: NamespaceGroup,
    pub relevant_txs: HashMap<Bytes, MsgPayForData>,
}

impl DaService for CelestiaService {
    type FilteredBlock = FilteredCelestiaBlock;

    type Future<T> = Pin<Box<dyn Future<Output = Result<T, Self::Error>>>>;

    // type Address;

    type Error = Box<dyn std::error::Error>;

    fn get_finalized_at(height: usize) -> Self::Future<Self::FilteredBlock> {
        Box::pin(async move {
            let rpc_addr = format!("http://localhost:26659/headers/height/{}", height);
            let raw_response = reqwest::get(rpc_addr).await?.text().await?;
            let header_response: CelestiaHeaderResponse = serde_json::from_str(&raw_response)?;
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;

            let rpc_addr = format!(
                "http://localhost:26659/namespaced_shares/{}/height/{}",
                hex::encode(ROLLUP_NAMESPACE),
                height
            );

            let body = reqwest::blocking::get(rpc_addr)?.text()?;
            let response: NamespacedSharesResponse = serde_json::from_str(&body)?;
            let rollup_shares = NamespaceGroup::from_b64_shares(&response.shares)?;

            let rpc_addr = format!(
                "http://localhost:26659/namespaced_shares/0000000000000001/height/{}",
                height
            );

            let body = reqwest::blocking::get(rpc_addr)?.text()?;
            let response: NamespacedSharesResponse = serde_json::from_str(&body)?;
            let tx_data = NamespaceGroup::from_b64_shares(&response.shares)?;

            let pfds = parse_tx_namespace(tx_data)?;

            let mut pfd_map = HashMap::new();

            for tx in pfds {
                pfd_map.insert(tx.message_share_commitment.clone(), tx);
            }

            let filtered_block = FilteredCelestiaBlock {
                header: header_response.header,
                rollup_data: rollup_shares,
                relevant_txs: pfd_map,
            };

            Ok::<Self::FilteredBlock, Box<dyn std::error::Error>>(filtered_block)
        })
    }

    fn get_block_at(height: usize) -> Self::Future<Self::FilteredBlock> {
        Self::get_finalized_at(height)
    }
}

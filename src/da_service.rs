use std::{collections::HashMap, future::Future, pin::Pin};

use nmt_rs::{NamespaceId, NamespacedHash};
use serde::Deserialize;
use sovereign_sdk::{da::DaService, Bytes};
use tendermint::merkle;

// 0x736f762d74657374 = b"sov-test"
// pub const ROLLUP_NAMESPACE: NamespaceId = NamespaceId(b"sov-test");
pub const ROLLUP_NAMESPACE: NamespaceId = NamespaceId([115, 111, 118, 45, 116, 101, 115, 116]);
pub const TRANSACTIONS_NAMESPACE: NamespaceId = NamespaceId(hex_literal::hex!("0000000000000001"));
pub const PARITY_SHARES_NAMESPACE: NamespaceId = NamespaceId(hex_literal::hex!("ffffffffffffffff"));

use crate::{
    parse_tx_namespace,
    payment::MsgPayForData,
    shares::{NamespaceGroup, Share},
    CelestiaHeader, CelestiaHeaderResponse, DataAvailabilityHeader, NamespacedSharesResponse,
    TxPosition,
};

#[derive(Debug, Clone)]
pub struct CelestiaService;

#[derive(Debug, Clone, PartialEq)]
pub struct FilteredCelestiaBlock {
    pub header: CelestiaHeader,
    pub rollup_data: NamespaceGroup,
    pub relevant_txs: HashMap<Bytes, (MsgPayForData, TxPosition)>,
    pub relevant_rows: Vec<RelevantRow>,
}

#[derive(Debug, Copy, Clone, PartialEq)]
pub enum ValidationError {
    MissingDataHash,
    InvalidDataRoot,
    InvalidEtxProof,
    MissingTx,
    InvalidSigner,
}

impl CelestiaHeader {
    pub fn validate_dah(&self) -> Result<(), ValidationError> {
        let rows_iter = self.dah.row_roots.iter();
        let cols_iter = self.dah.column_roots.iter();
        let byte_vecs = rows_iter
            .chain(cols_iter)
            .map(|hash| hash.0.to_vec())
            .collect();
        let root = merkle::simple_hash_from_byte_vectors(byte_vecs);
        let data_hash = self
            .header
            .data_hash
            .ok_or(ValidationError::MissingDataHash)?;
        if &root != data_hash.as_ref() {
            return Err(ValidationError::InvalidDataRoot);
        }
        Ok(())
    }
}

impl DaService for CelestiaService {
    type FilteredBlock = FilteredCelestiaBlock;

    type Future<T> = Pin<Box<dyn Future<Output = Result<T, Self::Error>>>>;

    // type Address;

    type Error = Box<dyn std::error::Error>;

    fn get_finalized_at(height: usize) -> Self::Future<Self::FilteredBlock> {
        Box::pin(async move {
            let rpc_addr = format!("http://localhost:26659/header/{}", height);
            let raw_response = //if height != 45963 {
                reqwest::get(rpc_addr).await?.text().await?;
            let header_response: CelestiaHeaderResponse = serde_json::from_str(&raw_response)?;

            let rpc_addr = format!(
                "http://localhost:26659/namespaced_shares/{}/height/{}",
                hex::encode(ROLLUP_NAMESPACE),
                height
            );

            let body = reqwest::get(rpc_addr).await?.text().await?;
            let response: NamespacedSharesResponse = serde_json::from_str(&body)?;
            let rollup_shares =
                NamespaceGroup::from_b64_shares(&response.shares.unwrap_or_default())?;

            let rpc_addr = format!(
                "http://localhost:26659/namespaced_shares/0000000000000001/height/{}",
                height
            );

            let body = reqwest::get(rpc_addr).await?.text().await?;
            let response: NamespacedSharesResponse = serde_json::from_str(&body)?;
            let tx_data = NamespaceGroup::from_b64_shares(&response.shares.unwrap_or_default())?;

            let pfds = parse_tx_namespace(tx_data)?;

            let mut pfd_map = HashMap::new();

            for tx in pfds {
                pfd_map.insert(tx.0.message_share_commitment.clone(), tx);
            }

            let dah = header_response.dah.try_into()?;
            let relevant_rows = get_relevant_rows(height, ROLLUP_NAMESPACE, &dah).await?;

            // let original_rows = &header_response.dah.row_roots;
            let filtered_block = FilteredCelestiaBlock {
                header: CelestiaHeader {
                    header: header_response.header,
                    dah,
                },
                rollup_data: rollup_shares,
                relevant_txs: pfd_map,
                relevant_rows: relevant_rows,
            };

            Ok::<Self::FilteredBlock, Box<dyn std::error::Error>>(filtered_block)
        })
    }

    fn get_block_at(height: usize) -> Self::Future<Self::FilteredBlock> {
        Self::get_finalized_at(height)
    }
}

#[derive(Deserialize, Clone, Debug)]
pub struct SharesResponse {
    shares: Vec<Vec<Share>>,
    // height: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RelevantRow {
    pub row: Vec<Share>,
    pub root: NamespacedHash,
}

async fn get_relevant_rows(
    height: usize,
    nid: NamespaceId,
    dah: &DataAvailabilityHeader,
) -> Result<Vec<RelevantRow>, Box<dyn std::error::Error>> {
    let rpc_addr = format!("http://localhost:26659/shares/height/{}", height);
    let resp = reqwest::get(rpc_addr).await?.text().await?;
    let response: SharesResponse = serde_json::from_str(&resp)?;
    let mut output = Vec::new();
    for (row, root) in response.shares.into_iter().zip(dah.row_roots.iter()) {
        if root.contains(TRANSACTIONS_NAMESPACE) || root.contains(nid) {
            output.push(RelevantRow {
                row,
                root: root.clone(),
            })
        }
    }
    Ok(output)
}

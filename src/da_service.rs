use std::{collections::HashMap, future::Future, pin::Pin};

use nmt_rs::{NamespaceId, NamespacedHash};
use serde::{Deserialize, Serialize};
use sovereign_sdk::{da::DaService, Bytes};
use tendermint::merkle;

pub const ROLLUP_NAMESPACE: NamespaceId = NamespaceId(hex_literal::hex!("db841b1c364eb119"));

use crate::{
    parse_tx_namespace,
    payment::MsgPayForData,
    shares::{NamespaceGroup, Share},
    CelestiaHeader, CelestiaHeaderResponse, DataAvailabilityHeader,
    MarshalledDataAvailabilityHeader, NamespacedSharesResponse,
};

#[derive(Debug, Clone)]
pub struct CelestiaService;

#[derive(Debug, Clone, PartialEq)]
pub struct FilteredCelestiaBlock {
    pub header: CelestiaHeader,
    pub rollup_data: NamespaceGroup,
    pub relevant_txs: HashMap<Bytes, MsgPayForData>,
    pub relevant_rows: Vec<RelevantRow>,
}

#[derive(Debug, Copy, Clone, PartialEq)]
pub enum ValidationError {
    MissingDataHash,
    InvalidDataRoot,
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

            let body = if height != 45963
                && ROLLUP_NAMESPACE.0 == hex_literal::hex!("db841b1c364eb119")
            {
                reqwest::get(rpc_addr).await?.text().await?
            } else {
                r#"{"shares":["24QbHDZOsRkBiQMKmwIKAggLEgjbhBscNk6xGRj/ASDmnZWcBiogLxiynyPiv+8YkvcArwSevDEKL2a+kk09Omub1ZbBp0syIPZJYgbTRIKPNNj1pbebX6tyXw2A0FB/U/GsX/rn9PAbOiAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAEIgAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABKIAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAUiDjsMRCmPwcFJr79MiZb7kkJ65B5GSbk0yklZkbeFK4VVoUxKVJu7EdyHHyO0Zw/1ixbwdMUdhiIM1jkxSQTdKDa7fWbfgbkBAwowJFgoEy8prS5/4moiFCEgAaZwj+ARIgLxiynyPiv+8YkvcArwSevDEKL2a+kk09Omub1ZbBp0saQIxiJ3NCSIkro/jUWvNNF1PB8DuQBMBCLaW7LHheoEe20RZwxNyrLXVKTe9OyFlM23hiwCFhnE7ajQr9LoFQZwEAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA="],"height":45963}"#.to_string()
            };
            let response: NamespacedSharesResponse = serde_json::from_str(&body)?;
            let rollup_shares = NamespaceGroup::from_b64_shares(&response.shares)?;

            let rpc_addr = format!(
                "http://localhost:26659/namespaced_shares/0000000000000001/height/{}",
                height
            );

            let body = if height != 45963 {
                reqwest::get(rpc_addr).await?.text().await?
            } else {
                r#"{"shares":["AAAAAAAAAAEBxQIAAA8AwwIKIJn34lyyM0DcNJMnx76Ss326W3Nnq0ytTS9UO48y7UFdEpwCCnwKegoWL3BheW1lbnQuTXNnUGF5Rm9yRGF0YRJgCi9jZWxlc3RpYTFoNm1kem50dTVheGM1OGg5dnY2bnJhM2NmNDZ5Zm4zdzIybmU1YxII24QbHDZOsRkYiQMiIPKYFmkhhoKjg3HM/c5z1yg62hsf9UwRSE5fXf9shiQjEloKUQpGCh8vY29zbW9zLmNyeXB0by5zZWNwMjU2azEuUHViS2V5EiMKIQM/gcidsoTqnBX6AhGABRwiXOcbig/G4xB1UjDtpCwtDxIECgIIARjCAhIFEICb7gIaQEmQk+k+J1g3k5vpJOd/ypI0jcp43+GEaOcqQEGD5iHfYSsJ+qp5YFIBfwigZhLcS20yF+P38DaYmUAxqbZI6SkYAQAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA="],"height":45963}"#.to_string()
            };
            let response: NamespacedSharesResponse = serde_json::from_str(&body)?;
            let tx_data = NamespaceGroup::from_b64_shares(&response.shares)?;

            let pfds = parse_tx_namespace(tx_data)?;

            let mut pfd_map = HashMap::new();

            for tx in pfds {
                pfd_map.insert(tx.message_share_commitment.clone(), tx);
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
        if root.contains(nid) {
            output.push(RelevantRow {
                row,
                root: root.clone(),
            })
        }
    }
    Ok(output)
}

use std::{collections::HashMap, future::Future, pin::Pin};

use jsonrpsee::{core::client::ClientT, http_client::HttpClient};
use nmt_rs::NamespaceId;
use sovereign_sdk::services::da::DaService;
use tracing::{debug, info, span, Level};

// 0x736f762d74657374 = b"sov-test"
// pub const ROLLUP_NAMESPACE: NamespaceId = NamespaceId(b"sov-test");

use crate::{
    parse_pfb_namespace,
    share_commit::recreate_commitment,
    shares::{NamespaceGroup, Share},
    types::{ExtendedDataSquare, FilteredCelestiaBlock, Row, RpcNamespacedSharesResponse},
    utils::BoxError,
    verifier::{
        address::CelestiaAddress,
        proofs::{CompletenessProof, CorrectnessProof},
        CelestiaSpec, PFB_NAMESPACE, ROLLUP_NAMESPACE,
    },
    BlobWithSender, CelestiaHeader, CelestiaHeaderResponse, DataAvailabilityHeader,
};

#[derive(Debug, Clone)]
pub struct CelestiaService {
    client: HttpClient,
}

impl CelestiaService {
    pub fn with_client(client: HttpClient) -> Self {
        Self { client }
    }
}

impl CelestiaService {}

/// Fetch the rollup namespace shares and etx data. Returns a tuple `(rollup_shares, etx_shares)`
pub async fn fetch_needed_shares_by_header(
    client: &HttpClient,
    header: &serde_json::Value,
) -> Result<(NamespaceGroup, NamespaceGroup), BoxError> {
    let dah = header
        .get("dah")
        .ok_or(BoxError::msg("missing dah in block header"))?;
    let rollup_namespace_str = base64::encode(ROLLUP_NAMESPACE).into();
    let rollup_shares_future = {
        let params: Vec<&serde_json::Value> = vec![dah, &rollup_namespace_str];
        client.request::<RpcNamespacedSharesResponse, _>("share.GetSharesByNamespace", params)
    };

    let etx_namespace_str = base64::encode(PFB_NAMESPACE).into();
    let etx_shares_future = {
        let params: Vec<&serde_json::Value> = vec![dah, &etx_namespace_str];
        client.request::<RpcNamespacedSharesResponse, _>("share.GetSharesByNamespace", params)
    };

    let (rollup_shares_resp, etx_shares_resp) =
        tokio::join!(rollup_shares_future, etx_shares_future);

    let rollup_shares = NamespaceGroup::Sparse(
        rollup_shares_resp?
            .0
            .unwrap_or_default()
            .into_iter()
            .flat_map(|resp| resp.shares)
            .collect(),
    );
    let tx_data = NamespaceGroup::Compact(
        etx_shares_resp?
            .0
            .unwrap_or_default()
            .into_iter()
            .flat_map(|resp| resp.shares)
            .collect(),
    );

    Ok((rollup_shares, tx_data))
}

impl DaService for CelestiaService {
    type FilteredBlock = FilteredCelestiaBlock;

    type Spec = CelestiaSpec;

    type Future<T> = Pin<Box<dyn Future<Output = Result<T, Self::Error>>>>;

    type Error = BoxError;

    fn get_finalized_at(&self, height: u64) -> Self::Future<Self::FilteredBlock> {
        let client = self.client.clone();
        Box::pin(async move {
            let _span = span!(Level::TRACE, "fetching finalized block", height = height);
            // Fetch the header and relevant shares via RPC
            info!("Fetching header at height={}...", height);
            let header = client
                .request::<serde_json::Value, _>("header.GetByHeight", vec![height])
                .await?;
            debug!(header_result = ?header);
            info!("Fetching shares...");
            let (rollup_shares, tx_data) = fetch_needed_shares_by_header(&client, &header).await?;

            info!("Fetching EDS...");
            // Fetch entire extended data square
            let data_square = client
                .request::<ExtendedDataSquare, _>(
                    "share.GetEDS",
                    vec![header
                        .get("dah")
                        .ok_or(BoxError::msg("missing 'dah' in block header"))?],
                )
                .await?;

            let unmarshalled_header: CelestiaHeaderResponse = serde_json::from_value(header)?;
            let dah: DataAvailabilityHeader = unmarshalled_header.dah.try_into()?;
            info!("Parsing namespaces...");
            // Parse out all of the rows containing etxs
            let etx_rows =
                get_rows_containing_namespace(PFB_NAMESPACE, &dah, data_square.rows()?.into_iter())
                    .await?;
            // Parse out all of the rows containing rollup data
            let rollup_rows = get_rows_containing_namespace(
                ROLLUP_NAMESPACE,
                &dah,
                data_square.rows()?.into_iter(),
            )
            .await?;

            info!("Decoding pfb protofbufs...");
            // Parse out the pfds and store them for later retrieval
            let pfds = parse_pfb_namespace(tx_data)?;
            let mut pfd_map = HashMap::new();
            for tx in pfds {
                for (idx, nid) in tx.0.namespace_ids.iter().enumerate() {
                    if nid == &ROLLUP_NAMESPACE.0[..] {
                        // TODO: Retool this map to avoid cloning txs
                        pfd_map.insert(tx.0.share_commitments[idx].clone(), tx.clone());
                    }
                }
            }

            let filtered_block = FilteredCelestiaBlock {
                header: CelestiaHeader::new(dah, unmarshalled_header.header.into()),
                rollup_data: rollup_shares,
                relevant_pfbs: pfd_map,
                rollup_rows,
                pfb_rows: etx_rows,
            };

            Ok::<Self::FilteredBlock, BoxError>(filtered_block)
        })
    }

    fn get_block_at(&self, height: u64) -> Self::Future<Self::FilteredBlock> {
        self.get_finalized_at(height)
    }

    fn extract_relevant_txs(
        &self,
        block: Self::FilteredBlock,
    ) -> Vec<<Self::Spec as sovereign_sdk::da::DaSpec>::BlobTransaction> {
        let mut output = Vec::new();
        for blob in block.rollup_data.blobs() {
            let commitment =
                recreate_commitment(block.square_size(), blob.clone()).expect("blob must be valid");
            let sender = block
                .relevant_pfbs
                .get(&commitment[..])
                .expect("blob must be relevant")
                .0
                .signer
                .clone();

            let blob_tx = BlobWithSender {
                blob: blob.into(),
                sender: CelestiaAddress(sender.as_bytes().to_vec()),
            };
            output.push(blob_tx)
        }
        output
    }

    fn extract_relevant_txs_with_proof(
        &self,
        block: Self::FilteredBlock,
    ) -> (
        Vec<<Self::Spec as sovereign_sdk::da::DaSpec>::BlobTransaction>,
        <Self::Spec as sovereign_sdk::da::DaSpec>::InclusionMultiProof,
        <Self::Spec as sovereign_sdk::da::DaSpec>::CompletenessProof,
    ) {
        let relevant_txs = self.extract_relevant_txs(block.clone());
        let etx_proofs = CorrectnessProof::for_block(&block, &relevant_txs);
        let rollup_row_proofs = CompletenessProof::from_filtered_block(&block);

        (relevant_txs, etx_proofs.0, rollup_row_proofs.0)
    }
}

async fn get_rows_containing_namespace(
    nid: NamespaceId,
    dah: &DataAvailabilityHeader,
    data_square_rows: impl Iterator<Item = &[Share]>,
) -> Result<Vec<Row>, BoxError> {
    let mut output = vec![];

    for (row, root) in data_square_rows.zip(dah.row_roots.iter()) {
        if root.contains(nid) {
            output.push(Row {
                shares: row.to_vec(),
                root: root.clone(),
            })
        }
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use crate::{
        parse_pfb_namespace,
        shares::{NamespaceGroup, Share},
    };
    const SERIALIZED_PFB_SHARES: &'static str = r#"["AAAAAAAAAAQBAAABRQAAABHDAgq3AgqKAQqHAQogL2NlbGVzdGlhLmJsb2IudjEuTXNnUGF5Rm9yQmxvYnMSYwovY2VsZXN0aWExemZ2cnJmYXE5dWQ2Zzl0NGt6bXNscGYyNHlzYXhxZm56ZWU1dzkSCHNvdi10ZXN0GgEoIiCB8FoaUuOPrX2wFBbl4MnWY3qE72tns7sSY8xyHnQtr0IBABJmClAKRgofL2Nvc21vcy5jcnlwdG8uc2VjcDI1NmsxLlB1YktleRIjCiEDmXaTf6RVIgUVdG0XZ6bqecEn8jWeAi+LjzTis5QZdd4SBAoCCAEYARISCgwKBHV0aWESBDIwMDAQgPEEGkAhq2CzD1DqxsVXIriANXYyLAmJlnnt8YTNXiwHgMQQGUbl65QUe37UhnbNVrOzDVYK/nQV9TgI+5NetB2JbIz6EgEBGgRJTkRYAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA="]"#;
    const SERIALIZED_ROLLUP_DATA_SHARES: &'static str = r#"["c292LXRlc3QBAAAAKHsia2V5IjogInRlc3RrZXkiLCAidmFsdWUiOiAidGVzdHZhbHVlIn0AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA="]"#;

    #[test]
    fn test_get_pfbs() {
        // the following test case is taken from arabica-6, block 275345
        let shares: Vec<Share> =
            serde_json::from_str(SERIALIZED_PFB_SHARES).expect("failed to deserialize pfb shares");

        assert!(shares.len() == 1);

        let pfb_ns = NamespaceGroup::Compact(shares);
        let pfbs = parse_pfb_namespace(pfb_ns).expect("failed to parse pfb shares");
        assert!(pfbs.len() == 1);
    }

    #[test]
    fn test_get_rollup_data() {
        let shares: Vec<Share> = serde_json::from_str(SERIALIZED_ROLLUP_DATA_SHARES)
            .expect("failed to deserialize pfb shares");

        let rollup_ns_group = NamespaceGroup::Sparse(shares);
        let mut blobs = rollup_ns_group.blobs();
        let first_blob = blobs
            .next()
            .expect("iterator should contain exactly one blob");

        let found_data: Vec<u8> = first_blob.data().collect();
        assert!(&found_data == r#"{"key": "testkey", "value": "testvalue"}"#.as_bytes());

        assert!(blobs.next().is_none());
    }
}

use std::{collections::HashMap, future::Future, pin::Pin};

use borsh::{BorshDeserialize, BorshSerialize};
use jsonrpsee::{core::client::ClientT, http_client::HttpClient};
use nmt_rs::{CelestiaNmt, NamespaceId, NamespacedHash};
use serde::Deserialize;
use sovereign_sdk::{
    serial::{Decode, DecodeBorrowed, Encode},
    services::da::{DaService, SlotData},
    Bytes,
};
use tendermint::merkle;
use tracing::{debug, info, span, Level};

// 0x736f762d74657374 = b"sov-test"
// pub const ROLLUP_NAMESPACE: NamespaceId = NamespaceId(b"sov-test");
pub const ROLLUP_NAMESPACE: NamespaceId = NamespaceId([115, 111, 118, 45, 116, 101, 115, 116]);
pub const PFB_NAMESPACE: NamespaceId = NamespaceId(hex_literal::hex!("0000000000000004"));
pub const PARITY_SHARES_NAMESPACE: NamespaceId = NamespaceId(hex_literal::hex!("ffffffffffffffff"));

use crate::{
    parse_pfb_namespace,
    pfb::MsgPayForBlobs,
    shares::{NamespaceGroup, Share},
    types::{ExtendedDataSquare, RpcNamespacedSharesResponse},
    utils::BoxError,
    BlobWithSender, CelestiaHeader, CelestiaHeaderResponse, DataAvailabilityHeader, TxPosition,
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

#[derive(Debug, Clone, PartialEq, Deserialize, serde::Serialize)]
pub struct FilteredCelestiaBlock {
    pub header: CelestiaHeader,
    pub rollup_data: NamespaceGroup,
    /// A mapping from blob commitment to the PFB containing that commitment
    /// for each blob addressed to the rollup namespace
    pub relevant_pfbs: HashMap<Bytes, (MsgPayForBlobs, TxPosition)>,
    /// All rows in the extended data square which contain rollup data
    pub rollup_rows: Vec<Row>,
    /// All rows in the extended data square which contain pfb data
    pub pfb_rows: Vec<Row>,
}

impl Encode for FilteredCelestiaBlock {
    fn encode(&self, target: &mut impl std::io::Write) {
        serde_cbor::ser::to_writer(target, self).expect("serializing to writer should not fail");
    }
}

impl Decode for FilteredCelestiaBlock {
    type Error = anyhow::Error;

    fn decode<R: std::io::Read>(target: &mut R) -> Result<Self, <Self as Decode>::Error> {
        Ok(serde_cbor::de::from_reader(target)?)
    }
}

impl<'de> DecodeBorrowed<'de> for FilteredCelestiaBlock {
    type Error = anyhow::Error;

    fn decode_from_slice(target: &'de [u8]) -> Result<Self, Self::Error> {
        Ok(serde_cbor::de::from_slice(target)?)
    }
}

impl SlotData for FilteredCelestiaBlock {
    type BatchData = BlobWithSender;
    fn hash(&self) -> [u8; 32] {
        match self.header.header.hash() {
            tendermint::Hash::Sha256(h) => h,
            tendermint::Hash::None => unreachable!("tendermint::Hash::None should not be possible"),
        }
    }

    fn extra_data_for_storage(&self) -> Vec<u8> {
        serde_cbor::ser::to_vec(&self.header).expect("serializing to vec should not fail")
    }

    fn reconstruct_from_storage(extra_data: &[u8], batches: Vec<Self::BatchData>) -> Self {
        let header =
            serde_cbor::de::from_slice(extra_data).expect("deserializing from vec should not fail");
        let blobs: Vec<Share> = batches.into_iter().flat_map(|b| b.blob.0).collect();
        Self {
            header,
            rollup_data: NamespaceGroup::Sparse(blobs),
            relevant_pfbs: HashMap::new(),
            rollup_rows: Vec::new(),
            pfb_rows: Vec::new(),
        }
    }
}
impl FilteredCelestiaBlock {
    pub fn square_size(&self) -> usize {
        self.header.square_size()
    }

    pub fn get_row_number(&self, share_idx: usize) -> usize {
        share_idx / self.square_size()
    }
    pub fn get_col_number(&self, share_idx: usize) -> usize {
        share_idx % self.square_size()
    }

    pub fn row_root_for_share(&self, share_idx: usize) -> &NamespacedHash {
        &self.header.dah.row_roots[self.get_row_number(share_idx)]
    }

    pub fn col_root_for_share(&self, share_idx: usize) -> &NamespacedHash {
        &self.header.dah.column_roots[self.get_col_number(share_idx)]
    }
}

#[derive(Debug, Copy, Clone, PartialEq)]
pub enum ValidationError {
    MissingDataHash,
    InvalidDataRoot,
    InvalidEtxProof(&'static str),
    MissingTx,
    InvalidRowProof,
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
        if &root != <tendermint::Hash as AsRef<[u8]>>::as_ref(&data_hash) {
            return Err(ValidationError::InvalidDataRoot);
        }
        Ok(())
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

    type Future<T> = Pin<Box<dyn Future<Output = Result<T, Self::Error>>>>;

    // type Address;

    type Error = BoxError;

    fn get_finalized_at(&self, height: usize) -> Self::Future<Self::FilteredBlock> {
        let client = self.client.clone();
        Box::pin(async move {
            let _span = span!(Level::TRACE, "fetching finalized block", height = height);
            // Fetch the header and relevant shares via RPC
            info!("Fetching header...");
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
                        .ok_or(BoxError::msg("missing dah in block header"))?],
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
                header: CelestiaHeader {
                    header: unmarshalled_header.header,
                    dah,
                },
                rollup_data: rollup_shares,
                relevant_pfbs: pfd_map,
                rollup_rows,
                pfb_rows: etx_rows,
            };

            Ok::<Self::FilteredBlock, BoxError>(filtered_block)
        })
    }

    fn get_block_at(&self, height: usize) -> Self::Future<Self::FilteredBlock> {
        self.get_finalized_at(height)
    }
}

#[derive(
    Debug, Clone, PartialEq, serde::Serialize, Deserialize, BorshDeserialize, BorshSerialize,
)]
pub struct Row {
    pub shares: Vec<Share>,
    pub root: NamespacedHash,
}

impl Row {
    pub fn merklized(&self) -> CelestiaNmt {
        let mut nmt = CelestiaNmt::new();
        for (idx, share) in self.shares.iter().enumerate() {
            // Shares in the two left-hand quadrants are prefixed with their namespace, while parity
            // shares (in the right-hand) quadrants always have the PARITY_SHARES_NAMESPACE
            let namespace = if idx < self.shares.len() / 2 {
                share.namespace()
            } else {
                PARITY_SHARES_NAMESPACE
            };
            nmt.push_leaf(share.as_serialized(), namespace)
                .expect("shares are pushed in order");
        }
        assert_eq!(&nmt.root(), &self.root);
        nmt
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

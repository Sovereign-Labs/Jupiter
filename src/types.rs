use anyhow::ensure;
use serde::{Deserialize, Serialize};

use crate::{shares::Share, utils::BoxError};
#[derive(Deserialize, Serialize, PartialEq, Debug, Clone)]
pub struct RpcNamespacedShares {
    #[serde(rename = "Proof")]
    pub proof: JsonNamespaceProof,
    #[serde(rename = "Shares")]
    pub shares: Vec<Share>,
}

#[derive(Deserialize, Serialize, PartialEq, Debug, Clone)]
pub struct JsonNamespaceProof {
    #[serde(rename = "Start")]
    start: usize,
    #[serde(rename = "End")]
    end: usize,
    #[serde(rename = "Nodes")]
    nodes: Option<Vec<StringWrapper>>,
}

#[derive(Deserialize, Serialize, PartialEq, Debug, Clone)]
pub struct ExtendedDataSquare {
    pub data_square: Vec<Share>,
    pub codec: String,
}

impl ExtendedDataSquare {
    pub fn square_size(&self) -> Result<usize, BoxError> {
        let len = self.data_square.len();
        let square_size = (len as f64).sqrt() as usize;
        ensure!(
            square_size * square_size == len,
            "eds size {} is not a perfect square",
            len
        );
        Ok(square_size)
    }

    pub fn rows(&self) -> Result<Vec<&[Share]>, BoxError> {
        let square_size = self.square_size()?;

        let mut output = Vec::with_capacity(square_size);
        for i in 0..square_size {
            let row_start = i * square_size;
            let row_end = (i + 1) * square_size;
            output.push(&self.data_square[row_start..row_end])
        }
        Ok(output)
    }
}

#[derive(Deserialize, Serialize, PartialEq, Debug, Clone)]
pub struct StringWrapper {
    #[serde(rename = "/")]
    pub inner: String,
}

#[derive(Deserialize, Serialize, PartialEq, Debug, Clone)]
pub struct RpcNamespacedSharesResponse(pub Option<Vec<RpcNamespacedShares>>);

use nmt_rs::{
    simple_merkle::proof::Proof, NamespaceProof, NamespacedHash, NamespacedSha2Hasher,
    NAMESPACED_HASH_LEN,
};

impl Into<NamespaceProof<NamespacedSha2Hasher>> for JsonNamespaceProof {
    fn into(self) -> NamespaceProof<NamespacedSha2Hasher> {
        NamespaceProof::PresenceProof {
            proof: Proof {
                siblings: self
                    .nodes
                    .unwrap_or_default()
                    .into_iter()
                    .map(|v| ns_hash_from_b64(&v.inner))
                    .collect(),
                start_idx: self.start as u32,
            },
            ignore_max_ns: true,
        }
    }
}

fn ns_hash_from_b64(input: &str) -> NamespacedHash {
    let mut output = [0u8; NAMESPACED_HASH_LEN];
    base64::decode_config_slice(input, base64::STANDARD, &mut output[..])
        .expect("must be valid b64");
    NamespacedHash(output)
}

#[cfg(test)]
mod tests {

    use nmt_rs::{NamespaceProof, NamespacedSha2Hasher};

    use crate::da_service::ROLLUP_NAMESPACE;

    use super::{ns_hash_from_b64, RpcNamespacedSharesResponse};

    const ROW_ROOTS: &[&'static str] = &[
        "AAAAAAAAAAEAAAAAAAAAAT4A1HvHQCYkf1sQ7zmTJH11jd1Hxn+YCcC9mIGbl1WJ",
        "c292LXRlc3T//////////vSMLQPlgfwCOf4QTkOhMnQxk6ra3lI+ybCMfUyanYSd",
        "/////////////////////wp55V2JEu8z3LhdNIIqxbq6uvpyGSGu7prq67ajVVAt",
        "/////////////////////7gaLStbqIBiy2pxi1D68MFUpq6sVxWBB4zdQHWHP/Tl",
    ];

    #[test]
    fn test_known_good_msg() {
        let msg = r#"[{"Proof":{"End":1,"Nodes":[{"/":"bagao4amb5yatb7777777777773777777777777tjxe2jqsatxobgu3jqwkwsefsxscursxyaqzvvrxzv73aphwunua"},{"/":"bagao4amb5yatb77777777777777777777777776yvm54zu2vfqwyhd2nsebctxar7pxutz6uya7z3m2tzsmdtshjbm"}],"Start":0},"Shares":["c292LXRlc3QBKHsia2V5IjogInRlc3RrZXkiLCAidmFsdWUiOiAidGVzdHZhbHVlIn0AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA="]}]"#;
        let deserialized: RpcNamespacedSharesResponse =
            serde_json::from_str(msg).expect("message must deserialize");

        let root = ns_hash_from_b64(ROW_ROOTS[0]);

        for row in deserialized.0.expect("shares response is not empty") {
            let proof: NamespaceProof<NamespacedSha2Hasher> = row.proof.into();
            proof
                .verify_range(&root, &row.shares, ROLLUP_NAMESPACE)
                .expect("proof should be valid");
        }
    }
}

#[derive(Debug, PartialEq, Clone)]
pub struct BlobProof {
    pub proof: Vec<EtxRangeProof>,
}

#[derive(Debug, PartialEq, Clone)]
pub struct EtxRangeProof {
    pub shares: Vec<Vec<u8>>,
    pub proof: nmt_rs::Proof,
    pub start_share_idx: usize,
    pub start_offset: usize,
}

#[derive(Debug, PartialEq, Clone)]

pub struct RelevantRowProof {
    pub leaves: Vec<Vec<u8>>,
    pub proof: nmt_rs::Proof,
}

pub fn get_shares_for_tx() {}

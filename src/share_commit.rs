use crate::shares::{self, Share};

use self::nmt::build_nmt_from_po2_leaves;

use tendermint::merkle::simple_hash_from_byte_vectors;

/// Calculates the size of the smallest square that could be used to commit
/// to this message, following Celestia's "non-interactive default rules"
/// https://github.com/celestiaorg/celestia-app/blob/fbfbf111bcaa056e53b0bc54d327587dee11a945/docs/architecture/adr-008-blocksize-independent-commitment.md
fn min_square_size(message: &[u8]) -> usize {
    let square_size = message.len().next_power_of_two();
    if message.len() < (square_size * square_size - 1) {
        return square_size;
    } else {
        return square_size << 1;
    }
}

#[derive(Debug, Copy, Clone, PartialEq)]
pub enum CommitmentError {
    ErrMessageTooLarge,
}

/// Derived from https://github.com/celestiaorg/celestia-app/blob/0c81704939cd743937aac2859f3cb5ae6368f174/x/payment/types/payfordata.go#L170
pub fn recreate_commitment(
    square_size: usize,
    shares: shares::Blob,
) -> Result<[u8; 32], CommitmentError> {
    if shares.0.len() > (square_size * square_size) - 1 {
        return Err(CommitmentError::ErrMessageTooLarge);
    }

    let heights = power_of_2_mountain_range(shares.0.len(), square_size);
    let mut leaf_sets: Vec<&[Share]> = Vec::with_capacity(heights.len());
    let mut cursor = 0;
    for height in heights {
        leaf_sets.push(&shares.0[cursor..cursor + height]);
        cursor += height;
    }

    let mut subtree_roots = Vec::with_capacity(leaf_sets.len());
    for set in leaf_sets {
        let mut modified_set = Vec::new();
        for share in set {
            let mut prefixed = [0u8; 520];
            prefixed[..8].copy_from_slice(&share.as_ref()[..8]);
            prefixed[8..].copy_from_slice(share.as_ref());
            println!("nsleaf {:?}", &prefixed);
            modified_set.push(prefixed);
        }

        subtree_roots.push(build_nmt_from_po2_leaves(&modified_set));
    }
    dbg!(&subtree_roots);
    let h = simple_hash_from_byte_vectors(
        subtree_roots
            .into_iter()
            .map(|x| x.as_ref().to_vec())
            .collect(),
    );
    Ok(h)
}

// power_of_2_mountain_range returns the heights of the subtrees for binary merkle
// mountain range
fn power_of_2_mountain_range(mut len: usize, square_size: usize) -> Vec<usize> {
    let mut output = Vec::new();

    while len != 0 {
        if len >= square_size {
            output.push(square_size);
            len = len - square_size;
        } else {
            let p = next_lower_power_of_2(len);
            output.push(p);
            len = len - p;
        }
    }
    output
}

/// returns the largest power of 2 that is less than or equal to the input
/// Examples:
///   - next_lower_power_of_2(2): 2
///   - next_lower_power_of_2(3): 2
///   - next_lower_power_of_2(7): 4
///   - next_lower_power_of_2(8): 8
fn next_lower_power_of_2(num: usize) -> usize {
    if num.is_power_of_two() {
        num
    } else {
        num.next_power_of_two() >> 1
    }
}

mod nmt {
    use std::cmp::{max, min};

    use sovereign_sdk::core::crypto::hash::{sha2, sha2_merkle, Sha2Hash};

    #[derive(Debug, Copy, Clone, PartialEq)]
    pub enum Node<'a> {
        Leaf(&'a [u8]),
        Internal(&'a [u8]),
    }

    impl<'a> AsRef<[u8]> for Node<'a> {
        fn as_ref(&self) -> &[u8] {
            match self {
                Node::Leaf(x) => x,
                Node::Internal(x) => x,
            }
        }
    }

    impl<'a> Node<'a> {
        pub fn low_namespace(&self) -> &[u8] {
            match self {
                Node::Leaf(inner) => &inner[..8],
                Node::Internal(inner) => &inner[..8],
            }
        }
        pub fn high_namespace(&self) -> &[u8] {
            match self {
                Node::Leaf(inner) => &inner[..8],
                Node::Internal(inner) => &inner[8..16],
            }
        }
    }

    pub fn build_nmt_from_po2_leaves(leaves: &[impl AsRef<[u8]>]) -> [u8; 48] {
        if leaves.len() == 1 {
            let mut root = [0u8; 48];
            root[..8].copy_from_slice(&leaves[0].as_ref()[..8]);
            root[8..16].copy_from_slice(&leaves[0].as_ref()[..8]);

            root[16..].copy_from_slice(sha2_merkle(&[0], leaves[0].as_ref()).as_ref());
            dbg!(&root[16..]);
            return root;
        }
        let mut out = Vec::with_capacity(leaves.len() / 2);
        for [l, r] in leaves.array_chunks::<2>() {
            let left = Node::Leaf(l.as_ref());
            let right = Node::Leaf(r.as_ref());

            let mut root = [0u8; 48];
            root[..8].copy_from_slice(min(left.low_namespace(), right.low_namespace()));
            root[8..16].copy_from_slice(max(left.high_namespace(), right.high_namespace()));

            root[16..].copy_from_slice(sha2_merkle(left.as_ref(), right.as_ref()).as_ref());
            out.push(root)
        }
        if out.len() == 1 {
            return out[0];
        }
        return build_nmt_from_po2_inners(out);
    }

    fn build_nmt_from_po2_inners(inners: Vec<[u8; 48]>) -> [u8; 48] {
        todo!()
        // dbg!("Building nmt from inners with len", inners.len());
        // let mut out = Vec::with_capacity(inners.len() / 2);
        // for [l, r] in inners.array_chunks::<2>() {
        //     let left = Node::Internal(l);
        //     let right = Node::Internal(r);

        //     let mut root = [0u8; 48];
        //     root[..8].copy_from_slice(min(left.low_namespace(), right.low_namespace()));
        //     root[8..16].copy_from_slice(max(left.high_namespace(), right.high_namespace()));

        //     root[16..].copy_from_slice(sha2_merkle(left.as_ref(), right.as_ref()).as_ref());
        //     out.push(root)
        // }
        // if out.len() == 1 {
        //     return sha2(out[0].as_ref());
        // }
        // return build_nmt_from_po2_inners(out);
    }
}

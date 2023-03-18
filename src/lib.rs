#![feature(array_windows)]
#![feature(array_chunks)]
pub mod celestia;
pub mod shares;
pub use celestia::*;

pub mod da_app;
pub mod da_service;
pub mod share_commit;
pub mod types;
mod utils;

pub mod payment {
    include!(concat!(
        concat!(env!("OUT_DIR"), "/celestia"),
        "/payment.rs"
    ));
}

// pub mod payment {
//     include!(concat!(
//         concat!(env!("OUT_DIR"), "/celestia", "/gen"),
//         "/payment.rs"
//     ));
// }

pub mod tendermint {
    pub mod abci {
        include!(concat!(
            concat!(env!("OUT_DIR"), "/celestia-core", "/gen"),
            "/tendermint.abci.rs"
        ));
    }
    pub mod types {
        include!(concat!(
            concat!(env!("OUT_DIR"), "/celestia-core", "/gen"),
            "/tendermint.types.rs"
        ));
    }

    pub mod crypto {
        include!(concat!(
            concat!(env!("OUT_DIR"), "/celestia-core", "/gen"),
            "/tendermint.crypto.rs"
        ));
    }

    pub mod version {
        include!(concat!(
            concat!(env!("OUT_DIR"), "/celestia-core", "/gen"),
            "/tendermint.version.rs"
        ));
    }
}

// pub mod

pub mod cosmos {

    pub mod base {
        pub mod v1beta1 {
            include!(concat!(
                concat!(env!("OUT_DIR"), "/gen"),
                "/cosmos.base.v1beta1.rs"
            ));
        }
        pub mod abci {
            pub mod v1beta1 {
                include!(concat!(
                    concat!(env!("OUT_DIR"), "/gen"),
                    "/cosmos.base.abci.v1beta1.rs"
                ));
            }
        }
        pub mod query {
            pub mod v1beta1 {
                include!(concat!(
                    concat!(env!("OUT_DIR"), "/gen"),
                    "/cosmos.base.query.v1beta1.rs"
                ));
            }
        }
    }
    pub mod crypto {
        pub mod multisig {
            pub mod v1beta1 {
                include!(concat!(
                    concat!(env!("OUT_DIR"), "/gen"),
                    "/cosmos.crypto.multisig.v1beta1.rs"
                ));
            }
        }
    }

    pub mod tx {
        pub mod signing {
            pub mod v1beta1 {
                include!(concat!(
                    concat!(env!("OUT_DIR"), "/gen"),
                    "/cosmos.tx.signing.v1beta1.rs"
                ));
            }
        }
        pub mod v1beta1 {
            include!(concat!(
                concat!(env!("OUT_DIR"), "/gen"),
                "/cosmos.tx.v1beta1.rs"
            ));
        }
    }
}

// use snazzy::items;
// pub use payment::{MsgPayForData, MsgWirePayForData};

pub use cosmos::tx::v1beta1::Tx;

pub use crate::tendermint::types::MalleatedTx;

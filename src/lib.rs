pub mod celestia;
pub use celestia::*;

pub mod da_app;

// Include the `items` module, which is generated from items.proto.
// It is important to maintain the same structure as in the proto.
pub mod blob {
    include!(concat!(env!("OUT_DIR"), "/blob.rs"));
}

use snazzy::items;

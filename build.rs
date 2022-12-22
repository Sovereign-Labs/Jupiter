use std::env;
use std::error::Error;
use std::path::PathBuf;
use std::process::{exit, Command};

fn main() -> Result<(), Box<dyn Error>> {
    let current_dir: PathBuf = env!("CARGO_MANIFEST_DIR").into();
    let cosmos_sdk_dir = current_dir.join("cosmos-sdk").join("proto");
    // let status = Command::new("buf")
    //     .arg("generate")
    //     .arg("--template buf.gen.prost.yaml")
    //     .current_dir(&cosmos_sdk_dir)
    //     .status()
    //     .unwrap();

    // if !status.success() {
    //     exit(status.code().unwrap_or(-1))
    // }

    let out_dir: PathBuf = env::var("OUT_DIR").unwrap().into();
    // TODO!
    let status = Command::new("cp")
        .arg("-r")
        .arg(current_dir.join("celestia-app").join("rust").join("gen"))
        .arg(out_dir.join("celestia"))
        .status()
        .unwrap();
    if !status.success() {
        exit(status.code().unwrap_or(-1))
    }

    let status = Command::new("cp")
        .arg("-r")
        .arg(current_dir.join("celestia-core").join("rust").join("gen"))
        .arg(out_dir.join("celestia-core"))
        .status()
        .unwrap();
    if !status.success() {
        exit(status.code().unwrap_or(-1))
    }

    let status = Command::new("cp")
        .arg("-r")
        .arg(cosmos_sdk_dir.join("rust").join("gen"))
        .arg(env::var("OUT_DIR").unwrap())
        .status()
        .unwrap();
    if !status.success() {
        exit(status.code().unwrap_or(-1))
    }

    Ok(())
}

// use std::io::Result;

// fn main() -> Result<()> {
//     // prost_build::compile_protos(
//     //     &["celestia-app/proto/blob/tx.proto"],
//     //     &["celestia-app/proto/", "celestia-app/third_party/proto"],
//     // )?;
//     prost_build::compile_protos(
//         &["celestia-app/proto/payment/tx.proto"],
//         &["celestia-app/proto/", "celestia-app/third_party/proto"],
//     )?;
//     prost_build::compile_protos(
//         &["cosmos-sdk/proto/cosmos/tx/v1beta1/tx.proto"],
//         &[
//             "cosmos-sdk/proto/",
//             "cosmos-sdk/third_party/proto",
//             "celestia-app/third_party/proto",
//         ],
//     )?;
//     Ok(())
// }

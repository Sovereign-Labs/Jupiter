# Jupiter

Jupiter is a *research-only* adapter making Celestia compatible with the Sovereign SDK. None of its code is
suitable for production use. It contains known security flaws and numerous inefficiencies.

## Celestia Integration

The current version of Jupiter runs against Celestia-node version `v0.5.0-rc5`. This is the version used on the `arabica` testnet
as of Dec 1, 2022.

## Warning

Jupiter is a research prototype. It contains known vulnerabilities and should not be used in production under any
circumstances.

## Getting Started

### Compile Jupiter and its Protobuf Dependencies

1. Clone this repository. `cd jupiter`
1. Clone the celestia-app, celestia-core, and cosmos-sdk repositories (`git clone --depth 100 {repo_name}`)
1. Install the `buf` Protobuf management tool (`brew install bufbuild/buf/buf`)
1. Install the `prost` plugin for the Protobuf compiler (which generates Rust code from .proto files): `cargo install protoc-gen-prost`
1. Build the Celestia-app protobuf definitions: `cd celestia-app`, `cp ../example.buf.gen.prost.yaml buf.gen.prost.yaml`,
and `buf generate --template buf.gen.prost.yaml`
1. Follow the same steps to generate protobufs in the `cosmos-sdk/proto` and `celestia-core` repos.

The repository should now compile with `cargo build`!

### Set up Celestia

Set up a Celestia light node running on the Arabica testnet, and patch it to add the `shares` endpoint required by Jupiter.

1. Clone the repository: `git clone https://github.com/celestiaorg/celestia-node.git`.
1. `cd celestia-node`
1. Checkout the code at v0.6.1: `git reset --hard 3a58679ed84da966d01173f32780134c7b830594`
1. Apply the patch file provided by jupiter to celestia-node: `git apply ../jupiter/0001-Add-shares-endpoint.patch`
1. Build and install the celestia binary: `make go-install`
1. Build celestia's key management tool `make cel-key`
1. Initialize the node: `celestia light init`
1. Start the node: `celestia light start --core.ip https://rpc-mocha.pops.one:9090 --gateway`

Once your Celestia node is up and running, simply `cargo run` to test out the prototype.

## License

Licensed under the [Apache License, Version
2.0](./LICENSE).

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in this repository by you, as defined in the Apache-2.0 license, shall be
licensed as above, without any additional terms or conditions.

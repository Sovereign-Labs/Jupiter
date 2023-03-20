# Jupiter

Jupiter is a _research-only_ adapter making Celestia compatible with the Sovereign SDK. None of its code is
suitable for production use. It contains known security flaws and numerous inefficiencies.

## Celestia Integration

The current version of Jupiter runs against Celestia-node version `v0.7.1`. This is the version used on the `arabica` testnet
as of Mar 18, 2023.

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
1. Checkout the code at v0.7.1: `git checkout tags/v0.7.1`
1. Build and install the celestia binary: `make build && make go-install`
1. Build celestia's key management tool `make cel-key`
1. Initialize the node: `celestia light init --p2p.network arabica`
1. Start the node: `./celestia light start --core.ip https://limani.celestia-devops.dev --p2p.network arabica --gateway --rpc.port 11111`
1. Obtain a JWT for RPC access: `./celestia light auth admin --p2p.network arabica`
1. Copy the JWT and and save it in main.rs as `const NODE_AUTH_TOKEN`

Once your Celestia node is up and running, simply `cargo run` to test out the prototype.

## License

Licensed under the [Apache License, Version
2.0](./LICENSE).

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in this repository by you, as defined in the Apache-2.0 license, shall be
licensed as above, without any additional terms or conditions.

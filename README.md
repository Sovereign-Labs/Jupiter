# Jupiter

Jupiter is a *research-only* adapter making Celestia compatible with the Sovereign SDK. None of its code is
suitable for production use. It contains known security flaws and numerous inefficiencies.

## Celestia Integration

The current version of Jupiter runs against Celestia-node version `v0.5.0-rc5`. This is the version used on the `arabica` testnet
as of Dec 1, 2022.

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

Follow the steps [here](https://docs.celestia.org/nodes/light-node) to set up a Celestia light node running on the Arabica testnet.

Use the `celestia version` command to verify that your new light client is running Celestia version `v0.5.0-rc5`. If not, the
Juptier codebase may require changes to match upcoming tweaks to Celestia's data format.

Jupiter assumes that the local Celestia node is running its RPC server on port 26659, so use something like the following command
to start your Celestia node: `celestia light start --core.ip https://limani.celestia-devops.dev --gateway --gateway.port 26659`.

Once your Celestia node is up and running, simply `cargo run` to test out the prototype.

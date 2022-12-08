use std::io::Cursor;

use hex_literal::hex;
use prost::Message;
use sovereign_node::decode_varint;
use sovereign_node::payment::MsgPayForData;
use sovereign_node::share_commit::recreate_commitment;
use sovereign_node::shares::NamespaceGroup;
use sovereign_node::CelestiaHeaderResponse;
use sovereign_node::MalleatedTx;
use sovereign_node::Tx;
use sovereign_node::TxType;
// use sovereign_node::MsgPayForBlob;
use sovereign_node::skip_compact_share_header;
use sovereign_node::skip_varint;
use sovereign_node::test_compact_share_parsing;
// use sovereign_node::MsgPayForData;
// use sovereign_node::MsgWirePayForData;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // let rpc_addr = "http://localhost:26659/header/45000";
    // // http://localhost:26659/namespaced_data/0000000000000001/height/45963

    // let body = reqwest::get(rpc_addr).await?.text().await?;

    // let response: CeljjestiaHeaderResponse = serde_json::from_str(&body)?;
    // dbg!(response);
    parse_tx_namespace();
    let data = "24QbHDZOsRkBiQMKmwIKAggLEgjbhBscNk6xGRj/ASDmnZWcBiogLxiynyPiv+8YkvcArwSevDEKL2a+kk09Omub1ZbBp0syIPZJYgbTRIKPNNj1pbebX6tyXw2A0FB/U/GsX/rn9PAbOiAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAEIgAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABKIAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAUiDjsMRCmPwcFJr79MiZb7kkJ65B5GSbk0yklZkbeFK4VVoUxKVJu7EdyHHyO0Zw/1ixbwdMUdhiIM1jkxSQTdKDa7fWbfgbkBAwowJFgoEy8prS5/4moiFCEgAaZwj+ARIgLxiynyPiv+8YkvcArwSevDEKL2a+kk09Omub1ZbBp0saQIxiJ3NCSIkro/jUWvNNF1PB8DuQBMBCLaW7LHheoEe20RZwxNyrLXVKTe9OyFlM23hiwCFhnE7ajQr9LoFQZwEAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=";

    let group = NamespaceGroup::from_b64(&data)?;
    let blobs: Vec<sovereign_node::shares::Blob> = group.blobs().collect();
    for blob in blobs {
        println!("shares[0] = {:?}", &blob.0[0].as_ref());
        let data: Vec<u8> = blob.data().collect();
        println!("Data length: {}", data.len());
        println!("data = {:?}", &data);
        let hash = recreate_commitment(4, blob).expect("Commitment must be computable");
        print!("{}", hex::encode(&hash[..]))
    }

    // test_compact_share_parsing(&mut data);
    // skip_compact_share_header(&mut data);

    Ok(())
}

fn parse_tx_namespace() -> Result<(), Box<dyn std::error::Error>> {
    let data = "AAAAAAAAAAEBxQIAAA8AwwIKIJn34lyyM0DcNJMnx76Ss326W3Nnq0ytTS9UO48y7UFdEpwCCnwKegoWL3BheW1lbnQuTXNnUGF5Rm9yRGF0YRJgCi9jZWxlc3RpYTFoNm1kem50dTVheGM1OGg5dnY2bnJhM2NmNDZ5Zm4zdzIybmU1YxII24QbHDZOsRkYiQMiIPKYFmkhhoKjg3HM/c5z1yg62hsf9UwRSE5fXf9shiQjEloKUQpGCh8vY29zbW9zLmNyeXB0by5zZWNwMjU2azEuUHViS2V5EiMKIQM/gcidsoTqnBX6AhGABRwiXOcbig/G4xB1UjDtpCwtDxIECgIIARjCAhIFEICb7gIaQEmQk+k+J1g3k5vpJOd/ypI0jcp43+GEaOcqQEGD5iHfYSsJ+qp5YFIBfwigZhLcS20yF+P38DaYmUAxqbZI6SkYAQAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=";

    let group = NamespaceGroup::from_b64(&data)?;
    let blobs: Vec<sovereign_node::shares::Blob> = group.blobs().collect();
    for blob in blobs {
        let data: Vec<u8> = blob.data().collect();
        println!("Data length: {}", data.len());

        let mut data = std::io::Cursor::new(data);

        // skip_varint(&mut data).unwrap();
        let (len, _) = decode_varint(&mut data)
            .expect("Varint must be valid")
            .unwrap();

        dbg!("Tx len", len);
        let backup = data.clone();
        let tx = match MalleatedTx::decode(data) {
            Ok(malleated) => {
                // The hash length must be 32
                if malleated.original_tx_hash.len() != 32 {
                    TxType::Other(Tx::decode(backup)?)
                } else {
                    TxType::Pfd(malleated)
                }
            }
            Err(_) => TxType::Other(Tx::decode(backup)?),
        };

        dbg!(&tx);

        let sdk_tx = match tx {
            TxType::Pfd(malleated) => {
                let inner = malleated.tx.clone();
                Tx::decode(inner)?
            }
            TxType::Other(tx) => tx,
        };

        let body = sdk_tx.body.expect("transaction must have body");
        for msg in body.messages {
            dbg!(&msg.type_url);
            if msg.type_url == "/payment.MsgPayForData" {
                let pfd = MsgPayForData::decode(std::io::Cursor::new(msg.value))?;
                dbg!(&pfd);
                if pfd.signer == "celestia1h6mdzntu5axc58h9vv6nra3cf46yfn3w22ne5c" {
                    let commitment = pfd.message_share_commitment;
                    dbg!(hex::encode(commitment));
                }
            }
        }

        // assert_eq!(data_from_node, data);
    }
    Ok(())
}

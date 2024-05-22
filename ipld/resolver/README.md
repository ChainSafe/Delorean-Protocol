# IPLD Resolver

The IPLD Resolver is a Peer-to-Peer library which can be used to resolve arbitrary CIDs from subnets in InterPlanetary Consensus.

See the [docs](./docs/) for a conceptual overview.

## Usage

Please have a look at the [smoke test](./tests/smoke.rs) for an example of using the library.

The following snippet demonstrates how one would create a resolver instance and use it:

```rust
async fn main() {
  let config = Config {
      connection: ConnectionConfig {
          listen_addr: "/ip4/127.0.0.1/tcp/0".parse().unwrap(),
          expected_peer_count: 1000,
          max_incoming: 25,
          max_peers_per_query: 10,
          event_buffer_capacity: 100,
      },
      network: NetworkConfig {
          local_key: Keypair::generate_secp256k1(),
          network_name: "example".to_owned(),
      },
      discovery: DiscoveryConfig {
          static_addresses: vec!["/ip4/95.217.194.97/tcp/8008/p2p/12D3KooWC1EaEEpghwnPdd89LaPTKEweD1PRLz4aRBkJEA9UiUuS".parse().unwrap()]
          target_connections: 50,
          enable_kademlia: true,
      },
      membership: MembershipConfig {
          static_subnets: vec![],
          max_subnets: 10,
          publish_interval: Duration::from_secs(300),
          min_time_between_publish: Duration::from_secs(5),
          max_provider_age: Duration::from_secs(60),
      },
  };

  let store = todo!("implement BitswapStore and a Blockstore");

  let service = Service::new(config, store.clone());
  let client = service.client();

  tokio::task::spawn(async move { service.run().await });

  let cid: Cid = todo!("the CID we want to resolve");
  let subnet_id: SubnetID = todo!("the SubnetID from where the CID can be resolved");

  match client.resolve(cid, subnet_id).await.unwrap() {
    Ok(()) => {
      let _content: MyContent = store.get(cid).unwrap();
    }
    Err(e) => {
      println!("{cid} could not be resolved from {subnet_id}: {e}")
    }
  }
}
```

# Deploying a new IPC hierarchy

We recommend that you connect to the existing contracts on CalibrationNet. Nevertheless, this document provides instructions for deploying a new root contract.

## Install prerequisites

* Install the basic requirements for IPC (see [README](../../README.md#Prerequisites))

* Install Node.js [Ubuntu] ([details](https://github.com/nodesource/distributions))
```bash
curl -fsSL https://deb.nodesource.com/setup_18.x | sudo -E bash -
sudo apt-get install nodejs
```

* Get the Solidity actors and install dependencies
```bash
cd contracts
npm install
```

## Set up and fund an EVM account

* Connect Metamask to the parent network (for calibrationnet, use `https://api.calibration.node.glif.io/rpc/v1` as the RPC and `314159` as the chain id)

* Create a new account (or use an existing one)

* Export the corresponding private key according to [these steps](https://support.metamask.io/hc/en-us/articles/360015289632-How-to-export-an-account-s-private-key)


## Deploy the contracts

Once inside the repo, you'll need to populate the `.env.template` file with the private key of the address you provided with funds in the previous step, and the endpoint of the target network on which you want to deploy
```bash
export PRIVATE_KEY=<your_private_key>
export RPC_URL=https://api.calibration.node.glif.io/rpc/v1
```

In your currently open terminal, you'll need to load these variables into your environment so you can deploy the contracts.

```bash
source .env.template
make deploy-ipc NETWORK=calibrationnet
```

If the deployment is successful, you should receive an output similar to this one:

```
$ contracts/ops/deploy.sh localnet
[*] Deploying libraries
[*] Output libraries available in /home/workspace/pl/ipc-solidity-actors/scripts/libraries.out
[*] Populating deploy-gateway script
[*] Gateway script in /home/workspace/pl/ipc-solidity-actors/scripts/deploy-gateway.ts
[*] Gateway deployed:
{ Gateway: '<GATEWAY_ADDRESS>' }
[*] Output gateway address in /home/workspace/pl/ipc-solidity-actors/scripts/gateway.out
[*] Populating deploy-registry script
[*] Registry script in /home/workspace/pl/ipc-solidity-actors/scripts/deploy-registry.ts
No need to generate any newer typings.
Nothing to compile
No need to generate any newer typings.
Deploying contracts with account: <ACCOUNT> and balance: <BALANCE>
registry contract deployed to: <REGISTRY_ADDRESS>
[*] IPC actors successfully deployed
```

Keep the addresses of the gateway and the registry contracts deployed, as you will need them to configure the IPC agent.

>💡If instead of deploying IPC Solidity in Calibration, you want to test them in a local network, the only thing that you need to do is to configure the `RPC_URL` of your `.env` to point to the corresponding network's RPC endpoint, and `make deploy-ipc NETWORK=localnet`. You can also use `NETWORK=auto` to let the deployment scripts figure out the chain ID and all the information required to deploy IPC over the network that `RPC_URL` is pointing to.

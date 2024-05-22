# IPC Axelar Token Example

> Cross L1 chain atomic deposit of ERC20 tokens into token-supply subnets, via the Axelar network.

**⚠️ Disclaimer: The contracts and implementation herein have not been security-reviewed or audited.
They are considered bleeding-edge in terms of development.
IPC developers do not make any guarantees.
USE AT YOUR OWN RISK.**

## Motivation

The IPC stack enables you to convert an ERC20 residing on the parent network to native supply of a child subnet.
This promotes the token to a "native coin" status, and it's then used for transfers within the subnet, as well as for gas payments.

But what if the token resides on an L1 other than the root of the subnet?
Imagine an ERC20 token residing on the Ethereum L1 or Polygon _(foreign L1)_.
We'll call this the _canonical token_, with symbol `TOK`.
Then there is an IPC L2 subnet anchored on the Filecoin L1 _(root L1)_ wishing to adopt `TOK` as its supply source.

We can leverage the [Axelar Interchain Token Service (ITS)](https://interchain.axelar.dev/) to replicate the canonical token across L1s: from Ethereum/Polygon to Filecoin.
Doing so creates a remote token on Filecoin managed by the Axelar bridge.
We'll call it `filTOK`.

Using Axelar, users can move `TOK` freely between Ethereum/Polygon <> Filecoin (the L1s).
Every bridge transaction takes a few minutes to settle (depending on finality, approval, execution).
But once `TOK` arrives to Filecoin in the form of `filTOK`, users would have to sign yet another transaction (this time on Filecoin) to deposit the newly arrived tokens into the IPC L2 subnet.

In summary: sign a tx on the foreign L1, wait, switch network to the root L1, sign another tx.
This is too much work for users.

This repo facilitates atomic cross L1 transfers and deposits of tokens from a foreign L1, straight into IPC L2 subnets.

## Solution

![](./architecture.png)

## Contents

This example contains a duo of contracts leveraging Axelar's General Message Passing (GMP) via the ITS to conduct an atomic transfer and subnet deposit.
It assumes Polygon as its _foreign L1_, but this can easily be changed through parameters.

**`IpcTokenSender`:** This contract is deployed on the source L1. The user transacts with this contract by calling `fundSubnet()`, passing in the Axelar token ID, the subnet ID, the receipient address within the subnet, and the amount. This transfers the tokens to the destination L1 via the Axelar ITS, addressed to `IpcTokenHandler`, sending the subnet/recipient routing along for the `IpcTokenHandler` to use.

**`IpcTokenHandler`:** This contract is deployed on the destination L1. It implements Axelar's `InterchainTokenExecutable` interface, enabling it to accept and handle tokens and call data forwarded by the Axelar gateway. It proceeds to deposit the received tokens into designated subnet, crediting them to the desired recipient, by calling the IPC gateway.

## Usage

This example comes with some handy scripts to get you started quickly.
But first, you will need to deploy an Axelar Interchain Token in, e.g. Polygon, and replicate it to Filecoin.
You can do that via Axelar's [testnet](https://testnet.interchain.axelar.dev/) or [mainnet](https://interchain.axelar.dev/) ITS portal.
For more information, refer to the [Axelar docs](https://docs.axelar.dev/dev/send-tokens/interchain-tokens/create-token).

1. Copy `.env.example` to `.env`.
2. Adjust the parameters, including the origin and destination chains, token addresses (from the Axelar deployment), and private keys.
3. Deploy the handler contract to the destination chain. The script records the address in `out/addresses.json`, and other scripts automatically pick it up from there.
    ```bash
   $ make deploy-handler
   ```
4. Deploy the sender contract to the origin chain.
   ```bash
   $ make deploy-sender
   ```
5. Try it out. This is an interactive command.
    ```bash
   $ make deposit
    ```


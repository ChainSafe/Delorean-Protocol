#!/usr/bin/env bash

set -e

# Create test artifacts, which is basically the Tendermint genesis file.

KEYS_DIR=/data/keys
CMT_DIR=/data/${NODE_NAME}/cometbft
GENESIS_FILE=/data/genesis.json

# Create a genesis file
fendermint \
  genesis --genesis-file $GENESIS_FILE \
  new \
    --chain-name $FM_CHAIN_NAME \
    --base-fee 1000 \
    --timestamp 1680101412 \
    --power-scale 0

# Create test keys
mkdir -p $KEYS_DIR
for NAME in alice bob charlie dave; do
  fendermint key gen --out-dir $KEYS_DIR --name $NAME;
done

# Create an account
fendermint \
  genesis --genesis-file $GENESIS_FILE \
  add-account --public-key $KEYS_DIR/alice.pk \
              --balance 1000

# Create a multisig account
fendermint \
  genesis --genesis-file $GENESIS_FILE \
  add-multisig  --public-key $KEYS_DIR/bob.pk \
                --public-key $KEYS_DIR/charlie.pk \
                --public-key $KEYS_DIR/dave.pk \
                --threshold 2 --vesting-start 0 --vesting-duration 1000000 \
                --balance 3000

# Create some Ethereum accounts
for NAME in emily eric; do
  fendermint key gen --out-dir $KEYS_DIR --name $NAME;
  fendermint key into-eth --out-dir $KEYS_DIR --secret-key $KEYS_DIR/$NAME.sk --name $NAME-eth;
  fendermint \
    genesis --genesis-file $GENESIS_FILE \
    add-account --public-key $KEYS_DIR/$NAME.pk \
                --balance 1000 \
                --kind ethereum
done

# Add a validator
fendermint \
  genesis --genesis-file $GENESIS_FILE \
  add-validator --public-key $KEYS_DIR/bob.pk --power 1

# Enable IPC with some dummy values to test contract deployment.
fendermint \
  genesis --genesis-file $GENESIS_FILE \
  ipc gateway \
    --subnet-id /r0 \
    --bottom-up-check-period 10 \
    --msg-fee 10 \
    --majority-percentage 66

# Convert FM genesis to CMT
fendermint \
  genesis --genesis-file $GENESIS_FILE \
  into-tendermint --out $CMT_DIR/config/genesis.json

# Convert FM validator key to CMT
fendermint \
  key into-tendermint --secret-key $KEYS_DIR/bob.sk \
    --out $CMT_DIR/config/priv_validator_key.json

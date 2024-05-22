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

# Create some validators
mkdir -p $KEYS_DIR
for NAME in victoria veronica vivienne; do
  fendermint key gen --out-dir $KEYS_DIR --name $NAME;

  # Create Ethereum accounts for them.
  fendermint \
    genesis --genesis-file $GENESIS_FILE \
    add-account --public-key $KEYS_DIR/$NAME.pk \
                --balance 1000 \
                --kind ethereum

  # Convert FM validator key to CMT
  fendermint \
    key into-tendermint --secret-key $KEYS_DIR/$NAME.sk \
      --out $KEYS_DIR/$NAME.priv_validator_key.json

  # Convert FM validator key to ETH
  fendermint \
    key into-eth --out-dir $KEYS_DIR \
      --secret-key $KEYS_DIR/$NAME.sk --name $NAME-eth;
done

# Add a validator
VALIDATOR_NAME=victoria

fendermint \
  genesis --genesis-file $GENESIS_FILE \
  add-validator --public-key $KEYS_DIR/$VALIDATOR_NAME.pk --power 1

# Convert FM genesis to CMT
fendermint \
  genesis --genesis-file $GENESIS_FILE \
  into-tendermint --out $CMT_DIR/config/genesis.json

# Copy the default validator key
cp $KEYS_DIR/$VALIDATOR_NAME.priv_validator_key.json \
   $CMT_DIR/config/priv_validator_key.json

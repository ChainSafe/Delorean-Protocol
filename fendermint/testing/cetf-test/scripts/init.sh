#!/usr/bin/env bash

set -e

# Create test artifacts, which is basically the Tendermint genesis file.

KEYS_DIR=/data/keys
CMT_DIR=/data/${NODE_NAME}/cometbft
GENESIS_FILE=/data/genesis.json

echo "Initializing test artifacts in $CMT_DIR"

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
for NAME in veronica victoria vivienne volvo; do
  fendermint key gen --out-dir $KEYS_DIR --name $NAME;

  # Create Ethereum accounts for them.
  fendermint \
    genesis --genesis-file $GENESIS_FILE \
    add-account --public-key $KEYS_DIR/$NAME.pk \
                --balance 10000 
                # --kind ethereum

  # Convert FM validator key to CMT
  fendermint \
    key into-tendermint --secret-key $KEYS_DIR/$NAME.sk \
      --out $KEYS_DIR/$NAME.priv_validator_key.json
done

# Add a validator
VALIDATOR_NAME=veronica
VALIDATOR_NAME_1=victoria
VALIDATOR_NAME_2=vivienne
VALIDATOR_NAME_3=volvo

fendermint \
  genesis --genesis-file $GENESIS_FILE \
  add-validator --public-key $KEYS_DIR/$VALIDATOR_NAME.pk --power 1

fendermint \
  genesis --genesis-file $GENESIS_FILE \
  add-validator --public-key $KEYS_DIR/$VALIDATOR_NAME_1.pk --power 1
fendermint \
  genesis --genesis-file $GENESIS_FILE \
  add-validator --public-key $KEYS_DIR/$VALIDATOR_NAME_2.pk --power 1
fendermint \
  genesis --genesis-file $GENESIS_FILE \
  add-validator --public-key $KEYS_DIR/$VALIDATOR_NAME_3.pk --power 1

# Convert FM genesis to CMT
fendermint \
  genesis --genesis-file $GENESIS_FILE \
  into-tendermint --out $CMT_DIR/config/genesis.json


# Copy the default validator key
cp $KEYS_DIR/$VALIDATOR_NAME.priv_validator_key.json \
   $CMT_DIR/config/priv_validator_key.json

# Copy the default validator  priv key
cp $KEYS_DIR/$VALIDATOR_NAME.sk \
   $CMT_DIR/config/validator_key.sk

cp $KEYS_DIR/$VALIDATOR_NAME.pk \
   $CMT_DIR/config/validator_key.pk

# Copy the default bls key
cp $KEYS_DIR/$VALIDATOR_NAME.bls.sk \
   $CMT_DIR/config/bls_key.sk

# Copy the default bls key
cp $KEYS_DIR/$VALIDATOR_NAME.bls.pk \
   $CMT_DIR/config/bls_key.pk

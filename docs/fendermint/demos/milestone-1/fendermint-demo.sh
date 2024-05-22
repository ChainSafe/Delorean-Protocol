#0
cargo install --locked --path fendermint/app

#1

mkdir test-network

#2
fendermint genesis --genesis-file test-network/genesis.json new --network-name test --base-fee 1000 --timestamp 1680101412

#3
cat test-network/genesis.json

#4
mkdir test-network/keys
for NAME in alice bob charlie dave; do
  fendermint key gen --out-dir test-network/keys --name $NAME;
done

#5
ls test-network/keys
cat test-network/keys/alice.pk

#6
fendermint \
  genesis --genesis-file test-network/genesis.json \
  add-account --public-key test-network/keys/alice.pk \
              --balance 1000000000000000000

#7
fendermint \
  genesis --genesis-file test-network/genesis.json \
  add-multisig  --public-key test-network/keys/bob.pk \
                --public-key test-network/keys/charlie.pk \
                --public-key test-network/keys/dave.pk \
                --threshold 2 --vesting-start 0 --vesting-duration 1000000 \
                --balance 3000000000000000000

#8
cat test-network/genesis.json | jq .accounts

#9
fendermint \
  genesis --genesis-file test-network/genesis.json \
  add-validator --public-key test-network/keys/bob.pk --power 1

#10
rm -rf ~/.tendermint
tendermint init

#11
fendermint \
  genesis --genesis-file test-network/genesis.json \
  into-tendermint --out ~/.tendermint/config/genesis.json

#12
cat ~/.tendermint/config/genesis.json

#13
fendermint \
  key into-tendermint --secret-key test-network/keys/bob.sk \
    --out ~/.tendermint/config/priv_validator_key.json

#14
cat ~/.tendermint/config/priv_validator_key.json
cat test-network/keys/bob.pk

#15
make actor-bundle

#16
rm -rf ~/.fendermint/data
mkdir -p ~/.fendermint/data
cp -r ./fendermint/app/config ~/.fendermint/config
cp ./builtin-actors/output/bundle.car ~/.fendermint/bundle.car
cp ./actors/output/custom_actors_bundle.car ~/.fendermint/custom_actors_bundle.car

#17
fendermint run

#18
tendermint unsafe-reset-all
tendermint start

#19
ALICE_ADDR=$(fendermint key address --public-key test-network/keys/alice.pk)
BOB_ADDR=$(fendermint key address --public-key test-network/keys/bob.pk)

#20
fendermint rpc query actor-state --address $ALICE_ADDR

#21
STATE_CID=$(fendermint rpc query actor-state --address $ALICE_ADDR | jq -r .state.state)
fendermint rpc query ipld --cid $STATE_CID

#22
fendermint rpc transfer --secret-key test-network/keys/alice.sk --to $BOB_ADDR --sequence 0 --value 1000

#23
fendermint rpc query actor-state --address $BOB_ADDR | jq .state.balance

#24
make ../builtin-actors
fendermint \
  rpc fevm --secret-key test-network/keys/alice.sk --sequence 1 \
    create --contract ../builtin-actors/actors/evm/tests/contracts/SimpleCoin.bin

#25
fendermint \
  rpc fevm --secret-key test-network/keys/alice.sk --sequence 2 \
    invoke --contract <delegated-address>  \
          --method f8b2cb4f --method-args 000000000000000000000000ff00000000000000000000000000000000000064

#26
cargo run -p fendermint_rpc --release \
  --example simplecoin -- \
  --secret-key test-network/keys/alice.sk --verbose

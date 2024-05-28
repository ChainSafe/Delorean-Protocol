deploy_example:
    #!/usr/bin/env bash
    set -euxo pipefail

    # Deploy the CetfAPI Library contract
    API_ADDRESS=$(forge create --root ./contracts --rpc-url localhost:8545 --private-key `cat ./fendermint/testing/cetf-test/test-data/keys/emily-eth.sk` src/cetf/CetfAPI.sol:CetfAPI | grep "Deployed to:" | awk '{print $3}')

    # Deploy the example and link the library
    forge create --root ./contracts --rpc-url localhost:8545 --private-key `cat ./fendermint/testing/cetf-test/test-data/keys/emily-eth.sk` --libraries src/CetfAPI.sol:CetfAPI:$API_ADDRESS src/cetf/Example.sol:CetfExample

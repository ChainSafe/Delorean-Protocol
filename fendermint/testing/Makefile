TEST_CONTRACTS_DIR  = contracts
TEST_CONTRACTS_SOL  = $(shell find $(TEST_CONTRACTS_DIR) -type f -name "*.sol")
TEST_CONTRACTS_BIN = $(TEST_CONTRACTS_SOL:.sol=.bin)

.PHONY: all
all: \
	test-contracts

# Compile all Solidity test contracts.
# This could also be achieved with https://docs.rs/ethers/latest/ethers/solc/
.PHONY: test-contracts
test-contracts: $(TEST_CONTRACTS_BIN)

# Compile a Solidity test contract
$(TEST_CONTRACTS_DIR)/%.bin: $(TEST_CONTRACTS_DIR)/%.sol | solc
	solc --bin --bin-runtime --abi --storage-layout --hashes --overwrite $< -o $(TEST_CONTRACTS_DIR)

# Requirements checks.

.PHONY: solc
solc:
	@if [ -z "$(shell which solc)" ]; then \
		echo "Please install solc, the Solidity compiler. See https://github.com/crytic/solc-select"; \
		exit 1; \
	fi

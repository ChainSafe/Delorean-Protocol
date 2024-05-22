include .env
export

.PHONY: install
install:
	forge install # installs existing dependencies

.PHONY: deps
deps:
	forge install --no-commit \
		axelarnetwork/axelar-gmp-sdk-solidity@v5.6.4 \
		axelarnetwork/interchain-token-service@v1.2.1 \
		openzeppelin/openzeppelin-contracts@v5.0.1

.PHONY: help
help:
	@echo "\033[0;32mAvailable targets:\033[0m"
	@echo "deps               - Install Foundry dependencies"
	@echo "deploy-sender      - Deploy the token sender contract on Polygon Mumbai"
	@echo "deploy-handler     - Deploy the token handler contract on Filecoin Calibrationnet"
	@echo "deposit            - Deposit tokens into the subnet"

.PHONY: deploy-handler
deploy-handler:
	forge script ./script/Deploy.s.sol \
		--sig "deployTokenHandler()" \
		--rpc-url "${$(DEST_NETWORK)__RPC_URL}" \
		--broadcast \
		--skip-simulation

.PHONY: deploy-sender
deploy-sender:
	forge script ./script/Deploy.s.sol \
		--sig "deployTokenSender()" \
		--rpc-url "${$(ORIGIN_NETWORK)__RPC_URL}" \
		--broadcast \
		--skip-simulation

.PHONY: deposit
deposit:
	@echo "Please enter these details:"; \
	read -p "Beneficiary: " BENEFICIARY; \
  	read -p "Amount: " AMOUNT; \
	read -p "Token ID (as registered in Axelar): " TOKEN_ID; \
	read -p "Destination subnet root [314159]: " SUBNET_ROOT; \
	read -p "Destination subnet address: " SUBNET_ADDR; \
	read -p "Gas payment (in origin's native coin) [350000000000000000]: " GAS_PAYMENT; \
	forge script ./script/Deposit.s.sol \
		--rpc-url "${$(ORIGIN_NETWORK)__RPC_URL}" \
		--broadcast \
		--sig "run(bytes32,uint256,uint256,address,uint64,address)" \
		-vvv \
		$$TOKEN_ID $$AMOUNT $${GAS_PAYMENT:-350000000000000000} $$BENEFICIARY $${SUBNET_ROOT:-314159} $$SUBNET_ADDR
### Makefile for Contract Fox - Soroban Smart Contracts
# Usage:
#   make build              # build entire workspace
#   make build-contracts    # build all contract WASMs
#   make test               # run all tests
#   make deploy-testnet     # deploy contracts to testnet
#   make deploy-mainnet     # deploy contracts to mainnet
#   make fmt                # format code
#   make lint               # lint code with clippy
#   make clean              # clean build artifacts

# --- Configuration ---
WASM_TARGET ?= wasm32-unknown-unknown
RELEASE_FLAG ?= --release
NETWORK ?= testnet
CONTRACTS = donation-contract withdrawal-contract campaign-contract

.PHONY: all help build build-contracts bindings test deploy-testnet deploy-mainnet fmt lint clean


all: build

help:
	@echo "Contract Fox - Soroban Smart Contracts"
	@echo ""
	@echo "Available targets:"
	@echo "  build              Build the entire workspace"
	@echo "  build-contracts    Build all contract WASMs for Soroban"
	@echo "  bindings           Generate Rust and TypeScript contract bindings"
	@echo "  test               Run all tests (workspace + contracts)"
	@echo "  deploy-testnet     Deploy contracts to Soroban testnet"
	@echo "  deploy-mainnet     Deploy contracts to Soroban mainnet"
	@echo "  fmt                Format code with cargo fmt"
	@echo "  lint               Lint code with cargo clippy"
	@echo "  clean              Clean all build artifacts"
	@echo ""
	@echo "Configuration:"
	@echo "  NETWORK            Network for deployment (default: testnet)"
	@echo "  RELEASE_FLAG       Build flag for contracts (default: --release)"

build:
	cargo build --workspace

build-contracts:
	@echo "Building all contract WASMs..."
	@for contract in $(CONTRACTS); do \
		echo "Building $$contract..."; \
		rustup target add $(WASM_TARGET) 2>/dev/null || true; \
		cargo build -p $$contract --target $(WASM_TARGET) $(RELEASE_FLAG) || exit 1; \
	done
	@echo "All contracts built successfully"

bindings:
	@command -v soroban >/dev/null 2>&1 || (echo "soroban CLI not found; install via 'cargo install soroban-cli'"; exit 1)

	@mkdir -p sdk/bindings/rust
	@mkdir -p sdk/bindings/typescript

	@test -n "$$DONATION_CONTRACT_ID" || (echo "DONATION_CONTRACT_ID is required" && exit 1)
	@test -n "$$WITHDRAWAL_CONTRACT_ID" || (echo "WITHDRAWAL_CONTRACT_ID is required" && exit 1)
	@test -n "$$CAMPAIGN_CONTRACT_ID" || (echo "CAMPAIGN_CONTRACT_ID is required" && exit 1)

	@echo "Generating Rust bindings..."

	soroban contract bindings rust \
		--contract-id $$DONATION_CONTRACT_ID \
		--network $(NETWORK) \
		--output sdk/bindings/rust/donation.rs

	soroban contract bindings rust \
		--contract-id $$WITHDRAWAL_CONTRACT_ID \
		--network $(NETWORK) \
		--output sdk/bindings/rust/withdrawal.rs

	soroban contract bindings rust \
		--contract-id $$CAMPAIGN_CONTRACT_ID \
		--network $(NETWORK) \
		--output sdk/bindings/rust/campaign.rs

	@echo "Generating TypeScript bindings..."

	soroban contract bindings typescript \
		--contract-id $$DONATION_CONTRACT_ID \
		--network $(NETWORK) \
		--output sdk/bindings/typescript/donation.ts

	soroban contract bindings typescript \
		--contract-id $$WITHDRAWAL_CONTRACT_ID \
		--network $(NETWORK) \
		--output sdk/bindings/typescript/withdrawal.ts

	soroban contract bindings typescript \
		--contract-id $$CAMPAIGN_CONTRACT_ID \
		--network $(NETWORK) \
		--output sdk/bindings/typescript/campaign.ts

	@echo "Bindings generated successfully"

deploy-testnet: build-contracts
	@command -v soroban >/dev/null 2>&1 || (echo "soroban CLI not found; install via 'cargo install soroban-cli'"; exit 1)
	@echo "Deploying contracts to Soroban testnet..."
	@for contract in $(CONTRACTS); do \
		WASM_FILE="target/$(WASM_TARGET)/release/$${contract}.wasm"; \
		if [ -f "$$WASM_FILE" ]; then \
			echo "Deploying $$WASM_FILE..."; \
			soroban contract deploy --wasm "$$WASM_FILE" --network testnet || exit 1; \
		fi; \
	done
	@echo "All contracts deployed to testnet"

deploy-mainnet: build-contracts
	@command -v soroban >/dev/null 2>&1 || (echo "soroban CLI not found; install via 'cargo install soroban-cli'"; exit 1)
	@echo "Deploying contracts to Soroban mainnet..."
	@for contract in $(CONTRACTS); do \
		WASM_FILE="target/$(WASM_TARGET)/release/$${contract}.wasm"; \
		if [ -f "$$WASM_FILE" ]; then \
			echo "Deploying $$WASM_FILE..."; \
			soroban contract deploy --wasm "$$WASM_FILE" --network mainnet || exit 1; \
		fi; \
	done
	@echo "All contracts deployed to mainnet"

fund:
	@if [ -z "$(ADDR)" ]; then echo "Usage: make fund ADDR=G..."; exit 1; fi
	@if [ "$(NETWORK)" != "testnet" ]; then echo "Friendbot only available on testnet/futurenet"; exit 1; fi
	@echo "Funding $(ADDR) via Friendbot"
	@curl -sS "https://friendbot.stellar.org/?addr=$(ADDR)" || true

invoke:
	@command -v soroban >/dev/null 2>&1 || (echo "soroban CLI not found; install via 'cargo install soroban-cli'"; exit 1)
	@if [ -z "$(FUNC)" ]; then echo "Usage: make invoke FUNC=<method> [CONTRACT_ID=<id>] [ARGS='arg1 arg2']"; exit 1; fi
	@CONTRACT_ID=$${CONTRACT_ID:-$$(cat .contract_id 2>/dev/null || true)}; \
	if [ -z "$$CONTRACT_ID" ]; then echo "Contract ID not set and .contract_id missing"; exit 1; fi; \
	ARGS=$${ARGS:-}; \
	set -x; soroban contract invoke --id "$$CONTRACT_ID" --network $(NETWORK) --fn $(FUNC) --args $$ARGS

test:
	cargo test --workspace

fmt:
	cargo fmt --all

lint:
	cargo clippy --all-targets --all-features -- -D warnings

clean:
	cargo clean


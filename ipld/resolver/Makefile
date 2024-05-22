.PHONY: all build test lint license check-fmt check-clippy diagrams

CRATE   := ipc_ipld_resolver
PACKAGE := $(patsubst %, --package %, $(CRATE))

all: test build

build:
	cargo build -Z unstable-options --release

test:
	cargo test --release $(PACKAGE)

clean:
	cargo clean

lint: \
	check-fmt \
	check-clippy

check-fmt:
	cargo +nightly fmt $(PACKAGE) --check

check-clippy:
	cargo clippy $(PACKAGE) --tests --no-deps -- -D clippy::all

diagrams:
	$(MAKE) -C docs/diagrams

check-diagrams: diagrams
	if git diff --name-only docs/diagrams | grep .png; then \
		echo "There are uncommitted changes to the diagrams"; \
		exit 1; \
	fi

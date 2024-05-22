# Each major sub-repository in this monorepo has their own Makefiles;
# instead of making an even more compilicated common one, let's delegate to them.

default:
	cd contracts && make gen
	cargo build --release
	./target/release/ipc-cli --version
	./target/release/fendermint --version

SUBTREES := fendermint ipc ipld/resolver contracts

test: $(patsubst %, test/%, $(SUBTREES))

# Using `cd` instead of `-C` so $(PWD) is correct.
test/%:
	cd $* && make test

lint/%:
	cd $* && make lint || echo "$* lint failed"

license:
	./scripts/add_license.sh

lint: license $(patsubst %, lint/%, $(SUBTREES))

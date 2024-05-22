FENDERMINT_DIR   := $(PWD)/../../..
SCRIPTS_DIR      := $(PWD)/scripts

TESTNET_ID       := state-size
MANIFEST_FILE    := $(PWD)/$(TESTNET_ID).yaml
NODE_ID          := bench

# Using the common materializer directory so port ranges are kept track of.
MATERIALIZER_DIR := $(FENDERMINT_DIR)/testing/materializer/tests/docker-materializer-data/
ROCKSDB_DIR      := $(MATERIALIZER_DIR)/testnets/$(TESTNET_ID)/root/nodes/$(NODE_ID)/fendermint/data/rocksdb

# Create the materializer as testnet, so we can disable chain metadata.
FM_NETWORK := testnet

# By default test with keeping all state.
STATE_HIST_SIZE  ?= 0
# By default add the blockhash to the ledger.
PUSH_CHAIN_META  ?= true
COMPACTION_STYLE ?= level

# Measurement settings
MEASUREMENTS_SUFFIX      ?= ""
MEASUREMENTS_DIR         := $(PWD)/measurements
MEASUREMENTS_FILE        := $(MEASUREMENTS_DIR)/state-size-$(STATE_HIST_SIZE)-$(PUSH_CHAIN_META)-$(COMPACTION_STYLE)$(MEASUREMENTS_SUFFIX).jsonline
MEASUREMENTS_PERIOD_SECS := 60

MEASUREMENTS_JSON        = $(shell find $(MEASUREMENTS_DIR) -type f -name "*.jsonline")
MEASUREMENTS_PNG         = $(MEASUREMENTS_JSON:.jsonline=.png)
MEASUREMENTS_STATS       = $(MEASUREMENTS_JSON:.jsonline=.stats)


# Export all the above variables as env vars, so they are available to scripts.
export

# Create the manifest file, e.g.:
# make manifest STATE_HIST_SIZE=100
.PHONY: manifest
manifest:
	$(SCRIPTS_DIR)/make-manifest.sh

# Setup the test node.
.PHONY: setup
setup: manifest
	cd $(FENDERMINT_DIR) && \
	cargo run -q -p fendermint_app -- \
		materializer \
			--data-dir $(MATERIALIZER_DIR) \
			setup \
				--manifest-file $(MANIFEST_FILE)

# Teardown the test node.
.PHONY: teardown
teardown:
	cd $(FENDERMINT_DIR) && \
	cargo run -q -p fendermint_app -- \
		materializer \
			--data-dir $(MATERIALIZER_DIR) \
			teardown --testnet-id $(TESTNET_ID)

# Take periodic measurements.
.PHONY: measurements
measurements:
	$(SCRIPTS_DIR)/make-measurements.sh

# Render measurement charts.
.PHONY: plots
plots: $(MEASUREMENTS_PNG)

.PHONY: stats
stats: $(MEASUREMENTS_STATS)
	@for f in $(MEASUREMENTS_DIR)/*.stats; do \
		echo $$f; cat $$f; \
	done

# Render a specfic plot if the data changed.
$(MEASUREMENTS_DIR)/%.png: $(MEASUREMENTS_DIR)/%.jsonline $(SCRIPTS_DIR)/growth-plot.plt | jq gnuplot
	$(SCRIPTS_DIR)/growth-plot.sh $@ $<

$(MEASUREMENTS_DIR)/%.stats: $(MEASUREMENTS_DIR)/%.jsonline | jq
	$(SCRIPTS_DIR)/stats.sh $< > $@

# Requirements checks.

.PHONY: gnuplot
gnuplot:
	@if [ -z "$(shell which gnuplot)" ]; then \
		echo "Please install gnuplot. See http://www.gnuplot.info/"; \
		exit 1; \
	fi

.PHONY: jq
jq:
	@if [ -z "$(shell which jq)" ]; then \
		echo "Please install jq. See https://stedolan.github.io/jq/"; \
		exit 1; \
	fi

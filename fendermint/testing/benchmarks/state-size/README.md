# State Size Benchmark

The purpose of this benchmark is to establish a baseline for state growth.
The idea is to run a simple standalone node and measure the size of the RocksDB data store as the block height goes up.


## Take measurements

Let's use the [materializer](../../materializer/) to start a node.

The following commands is assumed to be executed in this directory.


### Start a node and take measurements

```bash
make setup measurements STATE_HIST_SIZE=15 PUSH_CHAIN_META=false MEASUREMENTS_PERIOD_SECS=15
```

### Create statistics and plots

The following will create `.stat` and `.png` files in the [measurements](./measurements/) directory:

```bash
make stats plots
```

For example:

```console
  ~/projects/consensuslab/ipc/fendermint/testing/benchmarks/state-size  …798-benchmark-state-size ⇣1 ⇡1 ?4 ···················································································  1.77.0 10:11:25
❯ make stats plots
/home/aakoshh/projects/consensuslab/ipc/fendermint/testing/benchmarks/state-size/scripts/stats.sh /home/aakoshh/projects/consensuslab/ipc/fendermint/testing/benchmarks/state-size/measurements/state-size-15-false.jsonline > /home/aakoshh/projects/consensuslab/ipc/fendermint/testing/benchmarks/state-size/measurements/state-size-15-false.stats
/home/aakoshh/projects/consensuslab/ipc/fendermint/testing/benchmarks/state-size/measurements/state-size-0-true.stats
{
  "block_height": 10278,
  "db_size_kb": 60740,
  "avg_growth_kb": 5.131840311587147
}
/home/aakoshh/projects/consensuslab/ipc/fendermint/testing/benchmarks/state-size/measurements/state-size-15-false.stats
{
  "block_height": 381,
  "db_size_kb": 8236,
  "avg_growth_kb": 0.6170212765957447
}
/home/aakoshh/projects/consensuslab/ipc/fendermint/testing/benchmarks/state-size/measurements/state-size-15-true.stats
{
  "block_height": 2630,
  "db_size_kb": 21200,
  "avg_growth_kb": 5.0248946763692075
}
/home/aakoshh/projects/consensuslab/ipc/fendermint/testing/benchmarks/state-size/scripts/growth-plot.sh /home/aakoshh/projects/consensuslab/ipc/fendermint/testing/benchmarks/state-size/measurements/state-size-15-false.png /home/aakoshh/projects/consensuslab/ipc/fendermint/testing/benchmarks/state-size/measurements/state-size-15-false.jsonline
```

The filename indicates the parameters with with the measurements were taken, e.g. `-0-true` means that all app state history
was preserved and the chain metadata was recoreded in the ledger.

### Stop the node

```bash
make teardown
```

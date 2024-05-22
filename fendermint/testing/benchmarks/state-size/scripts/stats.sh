#!/bin/bash

set -e

cat $1 | jq --slurp '
		  .[0]  as $first
		| .[-1] as $last
		| { block_height: $last.block_height,
		    db_size_kb: $last.db_size_kb,
				avg_growth_kb: (($last.db_size_kb - $first.db_size_kb) / ($last.block_height - $first.block_height))
		}'

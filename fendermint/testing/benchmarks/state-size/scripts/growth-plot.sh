#!/bin/bash

set -e

OUT=$1
IN=$2

DAT=$IN.dat
PLT=$(dirname $0)/$(basename $0 .sh).plt

rm -f $DAT

cat $IN \
  | jq -r "[.block_height, .db_size_kb] | @tsv" \
  >> $DAT

gnuplot \
  -e "filein='$DAT'" \
  -e "fileout='$OUT'" \
  $PLT

rm $DAT

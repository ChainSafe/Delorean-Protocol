#!/usr/bin/env bash
set -eu
set -o pipefail

coverages=$(sed -n '/^.*headerCovTableEntryLo">/p' ./coverage_report/index.html | sed -En 's/[^0-9]*([0-9]{1,3})[^0-9]*/\1 /gp' | sed 's/ [0-9]//g')

measurements[0]="lines"
measurements[1]="functions"
measurements[2]="branches"

threshold=30
i=0

for c in $coverages
do
   if [ $c -lt $threshold ]; then
      echo "${measurements[i]} coverage is too low: $c";
      exit 1;
   fi
   i=$((i+1))
done

echo "coverage test passed";
exit 0
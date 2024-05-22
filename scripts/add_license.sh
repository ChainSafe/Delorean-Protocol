#!/bin/bash
#
# Checks if the source code contains required license and adds it if necessary.
# Returns 1 if there was a missing license, 0 otherwise.
COPYRIGHT_TXT=$(dirname $0)/copyright.txt

# Any year is fine. We can update the year as a single PR in all files that have it up to last year.
PAT_PL=".*// Copyright 202(1|2)-202[0-9] Protocol Labs.*"
PAT_SPDX="/*// SPDX-License-Identifier: .*"

# Look at enough lines so that we can include multiple copyright holders.
LINES=4

# Ignore auto-generated code.
IGNORE=(
	"contracts/binding" "ext/merkle-tree-rs"
);

ignore() {
	file=$1
	for path in "${IGNORE[@]}"; do
		if echo "$file" | grep -q "$path"; then
			return 0
		fi
	done
	return 1
}

ret=0


# NOTE: When files are moved/split/deleted, the following queries would find and recreate them in the original place.
# To avoid that, first commit the changes, then run the linter; that way only the new places are affected.

# `git grep` works from the perspective of the current directory

# Look for files without headers.
for file in $(git grep --cached -Il '' -- '*.rs'); do
	if ignore "$file"; then
		continue
	fi
  header=$(head -$LINES "$file")
	if ! echo "$header" | grep -q -E "$PAT_SPDX"; then
		echo "$file was missing header"
		cat $COPYRIGHT_TXT "$file" > temp
		mv temp "$file"
		ret=1
	fi
done

# `git diff` works from the root's perspective.

# Look for changes that don't have the new copyright holder.
for file in $(git diff --diff-filter=d --name-only origin/main -- '*.rs'); do
	if ignore "$file"; then
		continue
	fi
  header=$(head -$LINES "$file")
	if ! echo "$header" | grep -q -E "$PAT_PL"; then
		echo "$file was missing Protocol Labs"
		head -1 $COPYRIGHT_TXT > temp
		cat "$file" >> temp
		mv temp "$file"
		ret=1
	fi
done

exit $ret

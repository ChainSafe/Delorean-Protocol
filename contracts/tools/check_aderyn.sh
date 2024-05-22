#!/usr/bin/env bash
set -eux
set -o pipefail

# Path to the report file
REPORT_FILE="./report.json"

# List of severities that make us fail
SEVERITIES=(critical high medium)

# List of vulnerability titles to ignore
IGNORE_TITLES=("Centralization Risk for trusted owners")

# Specific vulnerabilities to ignore with path and line number
declare -A IGNORE_SPECIFIC
IGNORE_SPECIFIC["src/lib/LibDiamond.sol:204:Unprotected initializer"]=1
IGNORE_SPECIFIC["src/lib/LibDiamond.sol:203:Unprotected initializer"]=1

containsElement() {
  local e match="$1"
  shift
  for e; do [[ "$e" == "$match" ]] && return 0; done
  return 1
}

# Read vulnerabilities from the report
readVulnerabilities() {
  level="$1"
  jq -c --argjson ignoreTitles "$(printf '%s\n' "${IGNORE_TITLES[@]}" | jq -R . | jq -s .)" ".${level}_issues.issues[] | select(.title as \$title | .instances[].contract_path as \$path | .instances[].line_no as \$line | \$ignoreTitles | index(\$title) | not)" $REPORT_FILE
}

# Main function to process the report
processReport() {
  local hasVulnerabilities=0

  for level in ${SEVERITIES[@]}; do
    while IFS= read -r vulnerability; do
      title=$(echo "$vulnerability" | jq -r ".title")
      path=$(echo "$vulnerability" | jq -r ".instances[].contract_path")
      line=$(echo "$vulnerability" | jq -r ".instances[].line_no")
      specificKey="${path}:${line}:${title}"

      if [[ ${IGNORE_SPECIFIC[$specificKey]+_} ]]; then
        echo "Ignoring specific vulnerability: $title at $path line $line"
      else
        echo "Found $level vulnerability: $title at $path line $line"
        hasVulnerabilities=1
      fi
    done < <(readVulnerabilities "$level")
  done

  return $hasVulnerabilities
}

# Process the report and exit with the code returned by processReport
processReport
exit $?
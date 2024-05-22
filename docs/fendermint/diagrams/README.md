# Diagrams

This directory contains [PlantUML](https://plantuml.com/) diagrams which are turned into images ready to be embedded into docs.

To render the images, run the following command:

```shell
make diagrams
```

## Automation

Adding the following script to `.git/hooks/pre-commit` automatically renders and checks in the images when we commit changes to their source diagrams. CI should also check that there are no uncommitted changes.

```bash
#!/usr/bin/env bash

# If any command fails, exit immediately with that command's exit status
set -eo pipefail

# Redirect output to stderr.
exec 1>&2

if git diff --cached --name-only  --diff-filter=d | grep .puml
then
  make diagrams
  git add docs/diagrams/*.png
fi
```

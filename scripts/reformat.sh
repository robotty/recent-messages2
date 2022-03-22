#!/usr/bin/env sh

set -e

if [ "${1-}" = "--check" ]; then
  echo "Checking formatting..."
  PRETTIER_OPTIONS="--check"
  CARGO_FMT_OPTIONS="-- --check"
else
  echo "Reformatting..."
  PRETTIER_OPTIONS="--write"
  CARGO_FMT_OPTIONS=""
fi

# NOTE when updating these commands, don't forget to update the same commands in the CI configurations
# in /.github/workflows too
./web/node_modules/.bin/prettier $PRETTIER_OPTIONS --ignore-path ".gitignore" "**/*.md" "**/*.tsx" "**/*.yml" "**/*.json"
cargo fmt $CARGO_FMT_OPTIONS

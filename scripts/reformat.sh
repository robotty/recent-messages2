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

./web/node_modules/.bin/prettier $PRETTIER_OPTIONS --ignore-path ".gitignore" "**/*.md" "**/*.js" "**/*.tsx" "**/*.yml" "**/*.json"
cargo fmt $CARGO_FMT_OPTIONS

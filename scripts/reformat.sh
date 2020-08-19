#!/usr/bin/env sh

set -e

./web/node_modules/.bin/prettier --write --ignore-path ".gitignore" "**/*.md" "**/*.js" "**/*.tsx" "**/*.yml" "**/*.json"
cargo fmt

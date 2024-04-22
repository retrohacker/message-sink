#!/bin/bash

set -x

# Fail on warnings
export RUSTFLAGS=-Dwarnings

status=0

CMDS=(
  "cargo fmt --check"
  "cargo check --all-targets"
  "cargo clippy"
  "cargo test --all-targets"
)

for CMD in "${CMDS[@]}"
do
  if ! eval "${CMD}"
  then
    status=1
  fi
done

echo "Script execution completed."
exit "${status}"

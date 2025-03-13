#!/bin/bash
## on push branch~=gh-readonly-queue/main/.*
## on pull_request

set -euxo pipefail

# prevent "fatal: gc is already running" errors
git config --global gc.autoDetach false

cargo fmt -- --check

./d ci
./d check

#!/bin/bash
## on push branch=main
## permission contents write
## permission_repo mspm0-data-generated

set -euxo pipefail

git clone --depth 1 https://github.com/embassy-rs/stm32-data-generated/ build -q
./d ci

COMMIT=$(git rev-parse HEAD)
cd build
git add data mspm0-metapac
git commit -m "Generated from mspm0-data $COMMIT" --allow-empty
git tag -a stm32-data-$COMMIT -m "Generated from mspm0-data $COMMIT"
git push --follow-tags

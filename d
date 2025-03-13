#!/usr/bin/env bash

set -e
cd $(dirname $0)
CMD=$1

# Revision of the data sources 
REV=b99e2eed84917d13ccb28a1a7090703d4eda76d1
shift

case "$CMD" in
    download-all)
        rm -rf ./sources/
        git clone https://github.com/mspm0-rs/mspm0-data-sources ./sources/ --recursive --shallow-submodules
        cd ./sources/
        git checkout $REV
    ;;
    install-chiptool)
        cargo install --git https://github.com/embassy-rs/chiptool
    ;;
    gen)
        rm -rf build/data
        cargo run --release --bin mspm0-data-gen
    ;;
    build-metapac)
        rm -rf build/mspm0-metapac
        cargo run --release --bin mspm0-metapac-gen
    ;;
    ci)
        [ -d sources ] || ./d download-all
        [ -d sources ] || ./d gen
        [ -d sources ] || ./d build-metapac
    ;;
    check)
        cargo build --release --manifest-path build/mspm0-metapac/Cargo.toml --features pac,metadata,mspm0c110x
        cargo build --release --manifest-path build/mspm0-metapac/Cargo.toml --features pac,metadata,mspm0g110x
        cargo build --release --manifest-path build/mspm0-metapac/Cargo.toml --features pac,metadata,mspm0g150x
        cargo build --release --manifest-path build/mspm0-metapac/Cargo.toml --features pac,metadata,mspm0g151x
        cargo build --release --manifest-path build/mspm0-metapac/Cargo.toml --features pac,metadata,mspm0g310x
        cargo build --release --manifest-path build/mspm0-metapac/Cargo.toml --features pac,metadata,mspm0g350x
        cargo build --release --manifest-path build/mspm0-metapac/Cargo.toml --features pac,metadata,mspm0g351x
        cargo build --release --manifest-path build/mspm0-metapac/Cargo.toml --features pac,metadata,mspm0l110x
        cargo build --release --manifest-path build/mspm0-metapac/Cargo.toml --features pac,metadata,mspm0l122x
        cargo build --release --manifest-path build/mspm0-metapac/Cargo.toml --features pac,metadata,mspm0l130x
        cargo build --release --manifest-path build/mspm0-metapac/Cargo.toml --features pac,metadata,mspm0l134x
        cargo build --release --manifest-path build/mspm0-metapac/Cargo.toml --features pac,metadata,mspm0l222x
    ;;
    *)
        echo "unknown command"
    ;;
esac

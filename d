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
        ./d download-all
        ./d gen
        ./d build-metapac
    ;;
    check)
        # MSPS
        cargo build --release --manifest-path build/mspm0-metapac/Cargo.toml --features pac,metadata,msps003f3
        cargo build --release --manifest-path build/mspm0-metapac/Cargo.toml --features pac,metadata,msps003f4

        # C Series
        cargo build --release --manifest-path build/mspm0-metapac/Cargo.toml --features pac,metadata,mspm0c1103
        cargo build --release --manifest-path build/mspm0-metapac/Cargo.toml --features pac,metadata,mspm0c1104

        # G Series
        cargo build --release --manifest-path build/mspm0-metapac/Cargo.toml --features pac,metadata,mspm0g1105
        cargo build --release --manifest-path build/mspm0-metapac/Cargo.toml --features pac,metadata,mspm0g1106
        cargo build --release --manifest-path build/mspm0-metapac/Cargo.toml --features pac,metadata,mspm0g1107

        cargo build --release --manifest-path build/mspm0-metapac/Cargo.toml --features pac,metadata,mspm0g1505
        cargo build --release --manifest-path build/mspm0-metapac/Cargo.toml --features pac,metadata,mspm0g1506
        cargo build --release --manifest-path build/mspm0-metapac/Cargo.toml --features pac,metadata,mspm0g1507

        cargo build --release --manifest-path build/mspm0-metapac/Cargo.toml --features pac,metadata,mspm0g1518
        cargo build --release --manifest-path build/mspm0-metapac/Cargo.toml --features pac,metadata,mspm0g1519

        cargo build --release --manifest-path build/mspm0-metapac/Cargo.toml --features pac,metadata,mspm0g3505
        cargo build --release --manifest-path build/mspm0-metapac/Cargo.toml --features pac,metadata,mspm0g3506
        cargo build --release --manifest-path build/mspm0-metapac/Cargo.toml --features pac,metadata,mspm0g3507

        cargo build --release --manifest-path build/mspm0-metapac/Cargo.toml --features pac,metadata,mspm0g3105
        cargo build --release --manifest-path build/mspm0-metapac/Cargo.toml --features pac,metadata,mspm0g3106
        cargo build --release --manifest-path build/mspm0-metapac/Cargo.toml --features pac,metadata,mspm0g3107

        cargo build --release --manifest-path build/mspm0-metapac/Cargo.toml --features pac,metadata,mspm0g3518
        cargo build --release --manifest-path build/mspm0-metapac/Cargo.toml --features pac,metadata,mspm0g3519

        # L Series
        cargo build --release --manifest-path build/mspm0-metapac/Cargo.toml --features pac,metadata,mspm0l1105
        cargo build --release --manifest-path build/mspm0-metapac/Cargo.toml --features pac,metadata,mspm0l1106

        cargo build --release --manifest-path build/mspm0-metapac/Cargo.toml --features pac,metadata,mspm0l1227
        cargo build --release --manifest-path build/mspm0-metapac/Cargo.toml --features pac,metadata,mspm0l1228

        cargo build --release --manifest-path build/mspm0-metapac/Cargo.toml --features pac,metadata,mspm0l1304
        cargo build --release --manifest-path build/mspm0-metapac/Cargo.toml --features pac,metadata,mspm0l1305
        cargo build --release --manifest-path build/mspm0-metapac/Cargo.toml --features pac,metadata,mspm0l1306

        cargo build --release --manifest-path build/mspm0-metapac/Cargo.toml --features pac,metadata,mspm0l1343
        cargo build --release --manifest-path build/mspm0-metapac/Cargo.toml --features pac,metadata,mspm0l1344
        cargo build --release --manifest-path build/mspm0-metapac/Cargo.toml --features pac,metadata,mspm0l1345
        cargo build --release --manifest-path build/mspm0-metapac/Cargo.toml --features pac,metadata,mspm0l1346

        cargo build --release --manifest-path build/mspm0-metapac/Cargo.toml --features pac,metadata,mspm0l2227
        cargo build --release --manifest-path build/mspm0-metapac/Cargo.toml --features pac,metadata,mspm0l2228
    ;;
    *)
        echo "unknown command"
    ;;
esac

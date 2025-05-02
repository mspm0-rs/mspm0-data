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
    extract-all)
        peri=$1
        shift
	echo $@

        rm -rf tmp/$peri
        mkdir -p tmp/$peri

        for f in `ls sources/svd`; do
	    if [[ $f != *.svd ]]; then
		continue
	    fi
            f=${f%".svd"}
            echo -n processing $f ...
            if chiptool extract-peripheral --svd sources/svd/$f.svd --peripheral $peri $@ > tmp/$peri/$f.yaml 2> tmp/$peri/$f.err; then
                rm tmp/$peri/$f.err
                echo OK
            else
                if grep -q 'peripheral not found' tmp/$peri/$f.err; then
                    echo No Peripheral
                else
                    echo OTHER FAILURE
                fi
                rm tmp/$peri/$f.yaml
            fi
        done
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

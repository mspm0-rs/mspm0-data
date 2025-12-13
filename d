#!/usr/bin/env bash

set -e
cd $(dirname $0)
CMD=$1

# Revision of the data sources 
REV=99668fa76f2b3bb81996c24512e494d2c3a8b344
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
        ./d check
    ;;
    check)
        # Iterate over each chip that was generated in metapac and build it.
        #
        # TODO: Parallelize this to speed up checks?
        for feature in build/mspm0-metapac/src/chips/*/; do
            feature=$(basename "$feature")
            cargo build --release --manifest-path build/mspm0-metapac/Cargo.toml --features pac,metadata,$feature
        done
    ;;
    *)
        echo "unknown command"
    ;;
esac

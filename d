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
        # Build every chip the metapac generated.
        #
        # Cargo holds an exclusive lock on its target directory for the length of a build, so
        # running these in parallel against one directory only makes them queue. The chips are
        # dealt out to JOBS shards instead, each with a target directory of its own: the
        # dependencies are built once per shard and the chips within a shard reuse them. The
        # directories sit under build/mspm0-metapac/ so that `build-metapac` clears them.
        #
        # JOBS=1 restores the sequential behaviour. Do not regenerate while this runs.
        jobs=${JOBS:-$(nproc)}
        chips=(build/mspm0-metapac/src/chips/*/)
        logs=$(mktemp -d)

        for shard in $(seq 0 $((jobs - 1))); do
            (
                export CARGO_TARGET_DIR="build/mspm0-metapac/target/shard-$shard"

                for i in $(seq "$shard" "$jobs" $((${#chips[@]} - 1))); do
                    feature=$(basename "${chips[i]}")

                    if cargo build --release --manifest-path build/mspm0-metapac/Cargo.toml \
                        --features pac,metadata,"$feature" > "$logs/$feature" 2>&1
                    then
                        rm "$logs/$feature"
                        echo "ok $feature"
                    else
                        echo "FAILED $feature"
                    fi
                done
            ) &
        done
        wait

        # A chip's log is left behind only if it failed to build.
        failures=$(ls "$logs")

        for feature in $failures; do
            echo
            echo "=== $feature"
            cat "$logs/$feature"
        done

        rm -rf "$logs"
        [ -z "$failures" ]
    ;;
    *)
        echo "unknown command"
    ;;
esac

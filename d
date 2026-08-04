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
    download-docs)
        # Fetch the PDFs the tools/ scripts read, into ./files/.
        #
        # Not part of `ci`: nothing in the build reads these. They are the source for the checked-in
        # data/operating_modes, data/timers and data/errata, and are needed only to regenerate those.
        #
        # Named after the last segment of the URL, which is the document the data is traceable to.
        mkdir -p ./files/
        grep -oE '^  (datasheet|errata|reference_manual)_url: \S+' data/parts.yaml | sort -u |
        while read -r kind url; do
            case "$kind" in
                datasheet_url:) suffix=datasheet ;;
                errata_url:) suffix=errata ;;
                *) suffix=trm ;;
            esac

            out="./files/${url##*/}_$suffix.pdf"
            if [ -s "$out" ]; then
                echo "have $out"
                continue
            fi

            echo "fetching $out"
            curl -fsSL -o "$out" "$url" || echo "FAILED $url"
        done
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

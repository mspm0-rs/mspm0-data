#!/bin/bash

if [ "$#" -ne 2 ]; then
    echo "Usage: $0 <input> <transform>"
    exit 1
fi

filename=$(basename "$2")
dir=$(dirname "$2")
output="$dir/transform_$filename"

chiptool transform --input $1 --transform $2 --output $output
chiptool check $output

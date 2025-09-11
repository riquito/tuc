#/usr/bin/env bash

set -euo pipefail

if ! command -v hyperfine &> /dev/null; then
    echo "Error: hyperfine is not installed. Please install it first."
    exit 1
fi

if ! command -v hck &> /dev/null; then
    echo "Error: hck is not installed. Please install it first."
    exit 1
fi

if ! command -v choose &> /dev/null; then
    echo "Error: choose is not installed. Please install it first."
    exit 1
fi

if ! command -v coreutils &> /dev/null; then
    echo "Error: coreutils is not installed. Please install it first."
    exit 1
fi

TMP_DIR="tmp"
if [[ ! -d tmp ]]; then
    mkdir tmp
fi

if [[ ! -f tmp/data.csv ]]; then
    echo "Downloading input data..."
    wget -q -S -O - https://archive.ics.uci.edu/ml/machine-learning-databases/00347/all_train.csv.gz \
    | gunzip \
    | head -n 1000000 \
    > tmp/data.csv
fi

cargo build --release


hyperfine --warmup 3 -m 5 --export-markdown single_char.md --show-output \
     './target/release/tuc -d , -f 1,8,19 tmp/data.csv > /dev/null' \
     './target/release/tuc -d , -f 1,8,19 --no-mmap tmp/data.csv > /dev/null' \
     'hck -Ld, -f1,8,19 tmp/data.csv > /dev/null' \
     'hck -Ld, -f1,8,19 --no-mmap tmp/data.csv > /dev/null' \
     'hck -d, -f1,8,19  tmp/data.csv > /dev/null' \
     'hck -d, -f1,8,19  --no-mmap tmp/data.csv > /dev/null' \
     'coreutils cut -d , -f 1,8,19 tmp/data.csv > /dev/null' \
     'choose -f ',' -i tmp/data.csv 0 7 18 > /dev/null' \
     "awk -F, '{print \$1, \$8, \$19}' tmp/data.csv > /dev/null" \
     'cut -d, -f1,8,19 tmp/data.csv > /dev/null'

./target/release/tuc -d, -f1: -r '   ' ./tmp/data.csv > ./tmp/data-multichar.txt
sed -i 's/# label/#label/' ./tmp/data-multichar.txt

hyperfine --warmup 3 -m 5 --export-markdown multi_char.md --show-output \
    "./target/release/tuc -d'   ' -f 1,8,19 ./tmp/data-multichar.txt > /dev/null" \
    "./target/release/tuc -d'   ' -f 1,8,19 --no-mmap ./tmp/data-multichar.txt > /dev/null" \
    "./target/release/tuc -e'\s+' -f 1,8,19 ./tmp/data-multichar.txt > /dev/null" \
    "./target/release/tuc -e'\s+' -f 1,8,19 --no-mmap ./tmp/data-multichar.txt > /dev/null" \
    "hck -Ld'   ' -f1,8,19 ./tmp/data-multichar.txt > /dev/null" \
    "hck -Ld'   ' -f1,8,19 --no-mmap ./tmp/data-multichar.txt > /dev/null" \
    "hck -d'   ' -f1,8,19 ./tmp/data-multichar.txt > /dev/null" \
    "hck -d'   ' --no-mmap -f1,8,19 ./tmp/data-multichar.txt > /dev/null" \
    "hck -d'\s+' -f1,8,19 ./tmp/data-multichar.txt > /dev/null" \
    "hck -d'\s+' -f1,8,19 --no-mmap ./tmp/data-multichar.txt > /dev/null" \
    "choose -f '   ' -i ./tmp/data-multichar.txt 0 7 18  > /dev/null" \
    "choose -f '[[:space:]]' -i ./tmp/data-multichar.txt 0 7 18  > /dev/null" \
    "choose -f '\s' -i ./tmp/data-multichar.txt 0 7 18  > /dev/null" \
    "awk -F' ' '{print \$1, \$8 \$19}' ./tmp/data-multichar.txt > /dev/null" \
    "awk -F'   ' '{print \$1, \$8, \$19}' ./tmp/data-multichar.txt > /dev/null" \
    "awk -F'[:space:]+' '{print \$1, \$8, \$19}' ./tmp/data-multichar.txt > /dev/null" \
    "< ./tmp/data-multichar.txt tr -s ' ' | cut -d ' ' -f1,8,19 > /dev/null" \
    "< ./tmp/data-multichar.txt tr -s ' ' | hck -Ld' ' -f1,8,19 > /dev/null"

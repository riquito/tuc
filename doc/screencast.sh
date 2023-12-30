#!/bin/bash
# Designed to be executed via svg-term from the fd root directory:
# svg-term --command="bash doc/screencast.sh" --out doc/screencast.svg --padding=10
# Then run this (workaround for #1003):
# sed -i '' 's/<text/<text font-size="1.67"/g' doc/screencast.svg
set -e
set -u

PROMPT="\e[1;32m❯\e[0m"

enter() {
    INPUT="$1"
    DELAY=1

    prompt
    sleep "$DELAY"
    type "$INPUT"
    sleep 0.5
    printf '%b' "\\n"
    x=$(printf "$INPUT")
    eval "$x"
    type "\\n"
}

prompt() {
    printf '%b ' "$PROMPT" | pv -q
}

type() {
    printf "$1" | pv -qL $((10+(-2 + RANDOM%5)))
}

main() {
    IFS='%'

    enter "# Cut and rearrange fields..."
    enter "echo 'foo bar baz' | tuc -d ' ' -f 3,2,1"

    enter "# ...and join them back with the same delimiter"
    enter "echo 'foo bar baz' | tuc -d ' ' -f 3,2,1 -j"

    enter "# Replace the delimiter with something else"
    enter "echo 'foo bar baz' | tuc -d ' ' -r ' ➡ '"

    enter "# Keep a range of fields"
    enter "echo 'foo bar    baz' | tuc -d ' ' -f 2:"

    enter "# Indexes can be negative and rearranged"
    enter "echo 'a b c' | tuc -d ' ' -f -1,-2,-3"

    enter "# Cut using regular expressions"
    enter "echo 'a,b, c' | tuc -e '[, ]+' -f 1,3"

    enter "# Emit JSON output"
    enter "echo 'foo bar baz' | tuc -d ' ' --json"

    enter "# Delimiters can be any number of characters long"
    enter "echo 'a<sep>b<sep>c' | tuc -d '<sep>' -f 1,3"

    enter "# Cut using a greedy delimiter"
    enter "echo 'foo    bar' | tuc -d ' ' -f 1,2 -g"

    enter "# Format output"
    enter "echo 'foo bar baz' | tuc -d ' ' -f '{1}, {2} and lastly {3}'"

    enter "# ...with support for \\\\n"
    enter "echo '100Kb README.txt 2049-02-01' | tuc -d ' ' -f '{2}\\\\n├── {1}\\\\n└── {3}'"

    enter "# Cut lines (e.g. keep everything between first and last line)"
    enter "printf 'a\\\\nb\\\\nc\\\\nd\\\\ne' | tuc -l 2:-2"

    enter "# Concatenate lines (-l implies join with \\\\n, so we need --no-join)"
    enter "printf 'a\\\\nb\\\\nc\\\\nd\\\\ne' | tuc -l 1,2 --no-join"

    enter "# Compress delimiters after cut"
    enter "echo 'foo    bar   baz' | tuc -d ' ' -f 2: -p"

    enter "# Replace remaining delimiters with something else"
    enter "echo 'foo    bar   baz' | tuc -d ' ' -f 2: -p -r ' -> '"

    enter "# Cut characters (expects UTF-8 input)"
    enter "echo '😁🤩😝😎' | tuc -c 4,3,2,1"

    enter "# Cut bytes (the following emoji are 4 bytes each)"
    enter "echo '😁🤩😝😎' | tuc -b 5:8"

    enter "# Discard selected fields, keep the rest"
    enter "echo 'a b c' | tuc --complement -d ' ' -f 2"

    prompt

    sleep 3

    echo ""

    unset IFS
}

main

#!/bin/bash
# This script is a modified version of the one made by the good `fd` maintainers,
# see https://github.com/sharkdp/fd/blob/f227bb2/doc/screencast.sh
# Designed to be executed via svg-term from the root directory:
# svg-term --command="bash doc/screencast.sh" --out doc/screencast.svg --padding=10
# First copy doc/example in root (and delete it afterward).
# Then run this (workaround for https://bugzilla.mozilla.org/show_bug.cgi?id=1677988):
# sed -i '' 's/<text/<text font-size="1.67"/g' doc/screencast.svg
set -e
set -u

PROMPT="\e[1;32m❯\e[0m"

enter() {
    INPUT="$1"
    DELAY=1

    immediate_prompt
    sleep "$DELAY"
    type "$INPUT"
    sleep 0.5
    printf '%b' "\\n"
    # this is to ensure that we can have examples with \n and still display it
    x=$(printf "$INPUT")
    eval "$x"
}

comment() {
    INPUT="$1"
    DELAY=1

    prompt
    sleep "$DELAY"
    type "$INPUT"
    sleep 0.5
    type "\\n"
}

immediate_prompt() {
    printf '%b ' "$PROMPT"
}

prompt() {
    printf '%b ' "$PROMPT" | pv -q
    sleep 2 # give some time to read previous output
}

type() {
    printf "$1" | pv -qL $((10+(-2 + RANDOM%5)))
}

main() {
    IFS='%'

    comment "# Given this example file"
    enter "cat example"
    echo
    immediate_prompt
    sleep 5
    echo

    comment "# Say you want to keep only the 2nd element in each line"
    enter "cat example | tuc -d , -f 2"

    comment "# Let's keep just the last element of each  line"
    enter "cat example | tuc -d , -f -1"

    comment "# What if I wanted to skip first and last line?"
    enter "cat example | tuc -l 2:-2"

    comment "# Let's make it json"
    enter "cat example | tuc -d , --json"

    comment "# We can also format the output to do nifty things"
    comment "# Imagine you have .bak files and you want to rename them"
    comment "# (the file names would come from 'find' or similar tools)"
    enter "echo somefile.bak | tuc -d '.bak' -f 'mv {1}.bak {1}'"

    comment "# You can do much more than this. Check the documentation!"
    prompt

    sleep 3

    echo ""

    unset IFS
}

main

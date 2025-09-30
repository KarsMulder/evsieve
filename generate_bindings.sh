#!/bin/bash

cd "$(dirname "${BASH_SOURCE[0]}")"
cat licenses/thirdparty/libevdev/header.txt | sed 's/^/\/\/ /' > src/bindings/libevdev.rs 
bindgen --allowlist-type 'libevdev_.*' \
        --allowlist-function 'libevdev_.*' \
        --allowlist-var 'EV_.*' \
        --allowlist-var 'REP_.*' \
        --allowlist-var 'MSC_.*' \
        --no-doc-comments \
        src/bindings/libevdev.h \
        -- -I"/usr/include/libevdev-1.0/" \
        >> src/bindings/libevdev.rs

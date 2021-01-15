#!/bin/bash

cd "$(dirname "${BASH_SOURCE[0]}")"
cat licenses/thirdparty/libevdev/header.txt | sed 's/^/\/\/ /' > src/bindings/libevdev.rs 
bindgen --whitelist-type 'libevdev_.*' --whitelist-function 'libevdev_.*' --whitelist-var 'EV_.*' --whitelist-var 'REP_.*' src/bindings/libevdev.h -- -I"/usr/include/libevdev-1.0/" >> src/bindings/libevdev.rs

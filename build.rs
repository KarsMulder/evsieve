// SPDX-License-Identifier: GPL-2.0-or-later

fn main() {
    println!("cargo:rustc-link-lib=dylib=evdev");

    if let Ok(_library) = pkg_config::probe_library("systemd") {
        println!("cargo:rustc-cfg=feature=\"systemd\"");
        println!("cargo:rustc-link-lib=dylib=systemd");
    }
}
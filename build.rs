// SPDX-License-Identifier: GPL-2.0-or-later

fn main() {
    println!("cargo:rustc-link-lib=dylib=evdev");

    if cfg!(feature = "systemd") {
        println!("cargo:rustc-link-lib=dylib=systemd");
    }
}
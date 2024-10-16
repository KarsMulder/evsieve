#!/usr/bin/env python3

import os
import sys
import shutil
import subprocess as sp
import shlex
import libpkg

# Move the CWD to the root folder of the evsieve project.
git_root = libpkg.git_root
os.chdir(git_root)

libpkg.require_programs(["cargo", "rustc", "rpmbuild", "uname"])

# Compile evsieve
libpkg.compile_evsieve()

# Set up the package structure
rpm_root     = os.path.join(git_root, "target", "package", "rpm")
package_root = os.path.join(git_root, "target", "package", "rpm", "SOURCES", "evsieve")
package_dest = os.path.join(git_root, "target", "package", "evsieve.rpm")
if os.path.exists(package_root):
    shutil.rmtree(package_root)
if os.path.exists(package_dest):
    os.remove(package_dest)
os.makedirs(package_root)

install_evsieve(package_root)

# Set up the necessary meta-information that .deb packages require
current_architecture = sp.check_output(["uname", "-m"]).decode("utf-8").strip()
evsieve_version = libpkg.evsieve_version()

# TODO: do the fedora packaging standards expect us to include the following information like in Debian?
# ruststd_package = "libstd-rust-dev"
# ruststd_version = sp.check_output(["dpkg-query", "--showformat=${Version}", "--show", ruststd_package]).decode("utf-8").strip()


spec_info = f"""Name: evsieve
Version: {evsieve_version}
Release: 1
Summary: A utility for mapping events from Linux event devices
BuildArch: {current_architecture}
Source0: %{{name}}
License: GPL-2.0-or-later AND MIT AND GPL-2.0-only WITH Linux-syscall-note

%description
A utility for mapping events from Linux event devices
Evsieve (from "event sieve") is a low-level utility that can read events from Linux event devices (evdev) and write them to virtual event devices (uinput), performing simple manipulations on the events along the way. 

%prep

%build

%install
install -D -m 755 -o root -g root %{{SOURCE0}}usr/bin/evsieve ${{RPM_BUILD_ROOT}}%{{_bindir}}/evsieve

%files
usr/bin/evsieve
"""

spec_path = os.path.join(rpm_root, "evsieve.spec")
with open(spec_path, "wt") as file:
    file.write(spec_info)

# Compile the package
os.chdir(rpm_root)
sp.run(["rpmbuild", "-bb", spec_path]).check_returncode()

# TODO: Include license files in the generated package.

# TODO: print the path to the created package
# print(f"A .deb package file has been generated in: {package_dest}")
# print(f"To install it, run: sudo dpkg -i {shlex.quote(package_dest)}")

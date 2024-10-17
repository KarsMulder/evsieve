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
rpm_root     = os.path.join(git_root, "target", "package", "rpmbuild")
package_root = os.path.join(git_root, "target", "package", "rpmbuild", "SOURCES", "evsieve")
package_dest = os.path.join(git_root, "target", "package", "evsieve.rpm")
if os.path.exists(package_root):
    shutil.rmtree(package_root)
if os.path.exists(package_dest):
    os.remove(package_dest)
os.makedirs(package_root)

libpkg.install_evsieve(package_root)

# Set up the necessary meta-information that .deb packages require
current_architecture = sp.check_output(["uname", "-m"]).decode("utf-8").strip()
evsieve_version = libpkg.evsieve_version()

# TODO: do the fedora packaging standards expect us to include the following information like in Debian?
# ruststd_package = "libstd-rust-dev"
# ruststd_version = sp.check_output(["dpkg-query", "--showformat=${Version}", "--show", ruststd_package]).decode("utf-8").strip()

program_name = "evsieve"
release_number = 1
spec_info = f"""Name: {program_name}
Version: {evsieve_version}
Release: {release_number}
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
install -D -m 755 %{{SOURCE0}}/usr/bin/evsieve ${{RPM_BUILD_ROOT}}/%{{_bindir}}/evsieve

%files
/usr/bin/evsieve
"""

spec_path = os.path.join(rpm_root, "evsieve.spec")
with open(spec_path, "wt") as file:
    file.write(spec_info)

# Compile the package
os.chdir(rpm_root)
sp.run(["rpmbuild", "--define", f"_topdir {os.path.realpath(rpm_root)}", "-bb", spec_path]).check_returncode()

# Find the generated package
# target/package/rpmbuild/RPMS/x86_64/evsieve-1.4.0-1.x86_64.rpm
imputed_package_name = f"{program_name}-{evsieve_version}-{release_number}.{current_architecture}.rpm"
imputed_package_path = os.path.join(rpm_root, "RPMS", current_architecture, imputed_package_name)
desired_package_path = os.path.join(git_root, "target", imputed_package_name)
if os.path.exists(imputed_package_path):
    if os.path.exists(desired_package_path):
        os.unlink(desired_package_path)
    shutil.copy(imputed_package_path, desired_package_path)
    print(f"A .rpm package has been generated in: {desired_package_path}")
    print(f"To install it, run: sudo dnf install {shlex.quote(desired_package_path)}")
else:
    print(f"Error: an RPM package appears to have been generated, but not at the location we expected the package to be placed. It was expected to be at \"{imputed_package_path}\". Maybe you can find it somewhere near that place?")
    exit(1)

# TODO: Include license files in the generated package.


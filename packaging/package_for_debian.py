#!/usr/bin/env python3

import os
import sys
import shutil
import subprocess as sp
import shlex

# Move the CWD to the root folder of the evsieve project.
git_root = os.path.dirname(os.path.dirname(__file__))
os.chdir(git_root)

required_software = ["cargo", "rustc", "dpkg", "dpkg-query", "dpkg-deb"]
missing_software = []

# Returns True if a program with the given name is installed and available through PATH.
def has_program(program: str) -> bool:
    return (sp.run(["which", "--", program], stdin=sp.DEVNULL, stdout=sp.DEVNULL, stderr=sp.DEVNULL).returncode == 0)

for program in required_software:
    if not has_program(program):
        missing_software.append(program)

if missing_software:
    print("The following programs are required to compile and build a .deb package, but were not found in the PATH:", ", ".join(missing_software), file=sys.stderr)
    exit(1)

# Compile evsieve
sp.run(["cargo", "build", "--release"]).check_returncode()
executable_name = "evsieve"
executable_path = os.path.join(git_root, "target", "release", executable_name)

# Set up the package structure
package_root = os.path.join(git_root, "target", "package", "build", "deb")
package_dest = os.path.join(git_root, "target", "package", "evsieve.deb")
if os.path.exists(package_root):
    shutil.rmtree(package_root)
if os.path.exists(package_dest):
    os.remove(package_dest)
os.makedirs(package_root)

pkg_usr_bin = os.path.join(package_root, "usr", "bin")
install_path = os.path.join(pkg_usr_bin, executable_name)
os.makedirs(pkg_usr_bin)
shutil.copy(executable_path, install_path)
os.chmod(install_path, 0o755)

# Set up the necessary meta-information that .deb packages require
debian_path = os.path.join(package_root, "DEBIAN")
control_path = os.path.join(debian_path, "control")
copyright_path = os.path.join(debian_path, "copyright")
control_src_path = os.path.join(git_root, "packaging", "debian", "control")
os.makedirs(debian_path)

current_architecture = sp.check_output(["dpkg", "--print-architecture"]).decode("utf-8").strip()
evsieve_version = sp.check_output([executable_path, "--version"]).decode("utf-8").strip()

ruststd_package = "libstd-rust-dev"
ruststd_version = sp.check_output(["dpkg-query", "--showformat=${Version}", "--show", ruststd_package]).decode("utf-8").strip()


control_info = f"""Package: evsieve
Version: {evsieve_version}
Section: utils
Priority: optional
Architecture: {current_architecture}
Maintainer: Kars Mulder <devmail@karsmulder.nl>
Description: A utility for mapping events from Linux event devices
Depends: libevdev2
Built-Using: {ruststd_package} (= {ruststd_version})
"""

with open(control_path, "wt") as file:
    file.write(control_info)

# Include the copyright information
#
# TODO (LOW-PRIORITY): these licenses currently take up 116kB of data, which is like 10% of the size of
# the evsieve executable itself. That amount could be decreased by deduplicating licenses where possible
# (i.e. not including the GPL and Apache licence texts multiple times.)
copyright_info = """Format: https://www.debian.org/doc/packaging-manuals/copyright-format/1.0/
Source: https://github.com/KarsMulder/evsieve
Upstream-Name: evsieve
Upstream-Contact: Kars Mulder <devmail@karsmulder.nl>
License: GPL-2.0-or-later AND MIT AND GPL-2.0-only WITH Linux-syscall-note
Comment:
 Due to Debian packaging standards, several files from the development repository were merged together
 into this single file. Some of the text refers to "can be found in [path]". As such, we mention for
 each section what the original filename of that license on the development repository was. The file
 COPYING is the main file describing the copyright status of the evsieve project.

Files:
 usr/bin/evsieve
License:
"""

def format_copyright_info_from_file(path_from_root: str) -> str:
    result = f" --- Begin information taken from {path_from_root} ---\n"
    absolute_path = os.path.join(git_root, path_from_root)
    with open(absolute_path, "rt") as file:
        content = file.read()
    for line in content.splitlines(keepends=False):
        result += " " + line + "\n"
    result += f" --- End of file {path_from_root} ---\n"
    return result

def format_copyright_info_from_directory(path_from_root: str) -> str:
    result = ""
    for filename in sorted(os.listdir(os.path.join(git_root, path_from_root))):
        relative_path = os.path.join(path_from_root, filename)
        absolute_path = os.path.join(git_root, relative_path)
        if os.path.isfile(absolute_path):
            result += format_copyright_info_from_file(relative_path)
        elif os.path.isdir(absolute_path):
            result += format_copyright_info_from_directory(relative_path)
        else:
            raise Exception(f"Unhandled license file: {path_from_root}")

    return result

copyright_info += format_copyright_info_from_file("COPYING")
copyright_info += format_copyright_info_from_file("LICENSE")
copyright_info += format_copyright_info_from_directory("licenses")

with open(copyright_path, "wt") as file:
    file.write(copyright_info)

# Compile the package
sp.run(["dpkg-deb", "--build", os.path.abspath(package_root), os.path.abspath(package_dest)]).check_returncode()

print(f"A .deb package file has been generated in: {package_dest}")
print(f"To install it, run: sudo dpkg -i {shlex.quote(package_dest)}")

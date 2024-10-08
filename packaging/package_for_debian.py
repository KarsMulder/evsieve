#!/usr/bin/env python3

import os
import sys
import shutil
import subprocess as sp

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
if os.path.exist(package_dest):
    os.remove(package_dest)
os.makedirs(package_root)

pkg_usr_bin = os.path.join(package_root, "usr", "bin")
install_path = os.path.join(pkg_usr_bin, executable_name)
os.makedirs(pkg_usr_bin)
shutil.copy(executable_path, install_path)
os.chmod(install_path, 0o755)

debian_path = os.path.join(package_root, "DEBIAN")
control_path = os.path.join(debian_path, "control")
control_src_path = os.path.join(git_root, "packaging", "debian", "control")
os.makedirs(debian_path)


current_architecture = "amd64" #sp.check_output(["dpkg", "--check-architecture"]).decode("utf-8").strip()
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

sp.run(["dpkg-deb", "--build", os.path.abspath(package_root), os.path.abspath(package_dest)])

# TODO: Include license files in the generated package. Without the license files, it is legal
# to build and use the package, but not to distribute it.


# This file contains utility functions that are used by multiple packaging scripts.
import os
import sys
import subprocess as sp
import typing
import shutil

git_root = os.path.dirname(os.path.dirname(__file__))

# Returns True if a program with the given name is installed and available through PATH.
def has_program(program: str) -> bool:
    return (sp.run(["which", "--", program], stdin=sp.DEVNULL, stdout=sp.DEVNULL, stderr=sp.DEVNULL).returncode == 0)

# Checks that all programs are available. Prints and error and exits if any program is missing.
def require_programs(required_software: typing.List[str]):
    missing_software = []
    for program in required_software:
        if not has_program(program):
            missing_software.append(program)
    if missing_software:
        print("The following programs are required to compile and build a package, but were not found in the PATH:", ", ".join(missing_software), file=sys.stderr)
        exit(1)

executable_name = "evsieve"
executable_path = os.path.join(git_root, "target", "release", executable_name)

# Compiles evsieve. Requires the current working dir to be the git root.
def compile_evsieve():
    sp.run(["cargo", "build", "--release"]).check_returncode()

def install_evsieve(package_root: str):
    pkg_usr_bin = os.path.join(package_root, "usr", "bin")
    install_path = os.path.join(pkg_usr_bin, executable_name)
    os.makedirs(pkg_usr_bin)
    shutil.copy(executable_path, install_path)
    os.chmod(install_path, 0o755)

# Returns the version of evsieve that was compiled.
def evsieve_version():
    return sp.check_output([executable_path, "--version"]).decode("utf-8").strip()

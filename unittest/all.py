#!/usr/bin/env python3

import os
import sys
import subprocess as sp
import argparse

parser = argparse.ArgumentParser()
parser.add_argument("--binary", help="The evsieve binary to be tested", action='store', type=str, nargs='?', default="target/debug/evsieve")
args = parser.parse_args()

path_to_self = os.path.abspath(__file__)
self_filename = os.path.basename(path_to_self)
unittest_directory = os.path.dirname(path_to_self)

failed_tests = list()
for filename in os.listdir(unittest_directory):
    if filename == self_filename:
        continue
    unittest_path = os.path.join(unittest_directory, filename)
    result = sp.run(["python3", unittest_path, "--binary", args.binary]).returncode
    if result != 0:
        failed_tests.append(filename)
        print(f"The test {filename} failed.")

if failed_tests:
    print("!! Unittests failed !!")
    exit(1)



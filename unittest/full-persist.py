#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-or-later

import evdev
import evdev.ecodes as e
import os
import subprocess as sp
import time
import shutil
import typing
import argparse

parser = argparse.ArgumentParser()
parser.add_argument("--binary", help="The evsieve binary to be tested", action='store', type=str, nargs='?', default="target/debug/evsieve")
args = parser.parse_args()
EVSIEVE_PATH = args.binary

# Where we intend to put links to devices we use for this test.
TEST_DEVICE_PATH_IN_1 = "/dev/input/by-id/unittest-persist-in-1"
TEST_DEVICE_PATH_IN_2 = "/dev/input/by-id/unittest-persist-in-2"
TEST_DEVICE_PATH_OUT  = "/dev/input/by-id/unittest-persist-out"

# The name used for the systemd unit that runs evsieve for this test.
EVSIEVE_UNIT_NAME = "evsieve-unittest.service"

# EV_SYN capabilities are supported by all devices, whether you specify them or not.
DEFAULT_CAPS = {e.EV_SYN: [0, 1]}

# A temporary directory where evsieve should save its device cache for this test.
EVSIEVE_STATE_DIR = "/tmp/evsieve-unittest/full-persist"
if os.path.exists(EVSIEVE_STATE_DIR):
    shutil.rmtree(EVSIEVE_STATE_DIR)

# Make sure that our links are not already occupied.
if os.path.exists(TEST_DEVICE_PATH_OUT):
    sp.run(["systemctl", "stop", EVSIEVE_UNIT_NAME])
for path in [TEST_DEVICE_PATH_IN_1, TEST_DEVICE_PATH_IN_2, EVSIEVE_UNIT_NAME]:
    if os.path.exists(path):
        raise Exception(f"Cannot carry out the unittest: required path is already occupied.")
    if os.path.islink(path):
        os.unlink(path)

# Represents a virtual device we use for these tests.
class VirtualInputDevice:
    path: str
    capabilities: typing.Dict
    uinput: evdev.UInput | None

    def __init__(self, path, capabilities):
        self.path = path
        self.capabilities = capabilities
        self.uinput = None

    def open(self):
        self.uinput = evdev.UInput(self.capabilities)
        os.symlink(self.uinput.device, self.path)

    def close(self):
        self.uinput.close()
        os.unlink(self.path)
    
    def effective_capabilities(self):
        # Libevdev will automaticall add EV_SYN capabilities
        return self.capabilities | {e.EV_SYN: sorted([0] + [type for type in self.capabilities.keys()])}

# Creates an output device based on the provided input device.
# Output device is always TEST_DEVICE_PATH_OUT.
def start_evsieve(input_device_path: str):
    sp.run([
        "systemd-run",
        "-Gu", EVSIEVE_UNIT_NAME,
        "--service-type=notify",
        f"--property=Environment=EVSIEVE_STATE_DIR={EVSIEVE_STATE_DIR}",
        "--",
        EVSIEVE_PATH,
        "--input", input_device_path, "grab=force", "persist=full",
        "--output", f"create-link={TEST_DEVICE_PATH_OUT}",
    ])

def stop_evsieve():
    sp.run(["systemctl", "stop", EVSIEVE_UNIT_NAME])

# Queries the capabilities of the device at TEST_DEVICE_PATH_OUT.
def check_output_capabilities():
    output = evdev.InputDevice(TEST_DEVICE_PATH_OUT)
    capabilities = output.capabilities()
    output.close()
    return capabilities

# Checks if that evsieve properly mirrored the capabilities of the input device.
def has_matching_capabilities(device: VirtualInputDevice):
    return check_output_capabilities() == device.effective_capabilities()

# First, let's test if evsieve can properly mirror the capabilies of an input device.
test_device_alpha = VirtualInputDevice(TEST_DEVICE_PATH_IN_1, {e.EV_KEY: [e.KEY_A, e.KEY_C]})
test_device_beta  = VirtualInputDevice(TEST_DEVICE_PATH_IN_1, {e.EV_REL: [e.REL_X, e.REL_Y]})

test_device_alpha.open()
start_evsieve(TEST_DEVICE_PATH_IN_1)

assert(has_matching_capabilities(test_device_alpha))
assert(not has_matching_capabilities(test_device_beta))

# Let's check that evsieve retains those capabilities even after the device closes.
test_device_alpha.close()
time.sleep(0.1)

assert(has_matching_capabilities(test_device_alpha))
assert(not has_matching_capabilities(test_device_beta))

# ... and most importantly, retains those capabilities even after evsieve restarts.
assert(os.path.exists(TEST_DEVICE_PATH_OUT))
stop_evsieve()
assert(not os.path.exists(TEST_DEVICE_PATH_OUT))
start_evsieve(TEST_DEVICE_PATH_IN_1)
assert(os.path.exists(TEST_DEVICE_PATH_OUT))

assert(has_matching_capabilities(test_device_alpha))
assert(not has_matching_capabilities(test_device_beta))

# Now let's switch the capabilies of that device. Evsieve should detect this, destroy the output device,
# and re-create a new output device with the updated capabilities.
test_device_beta.open()
time.sleep(0.1)

assert(not has_matching_capabilities(test_device_alpha))
assert(has_matching_capabilities(test_device_beta))

# After restarting evsieve, the capabilities of device beta should be used instead of those of alpha.
test_device_beta.close()
stop_evsieve()
start_evsieve(TEST_DEVICE_PATH_IN_1)

assert(not has_matching_capabilities(test_device_alpha))
assert(has_matching_capabilities(test_device_beta))

# ... but if alpha is present instead of beta at startup, evsieve should use the capabilities of alpha again.
# (This differs from a previous test, because the previous set of test checked responding to changes in
# while evsieve was running, this one checks responding to changes since before startup.)
stop_evsieve()
test_device_alpha.open()
start_evsieve(TEST_DEVICE_PATH_IN_1)
assert(has_matching_capabilities(test_device_alpha))

# ... and the capabilities of alpha should've been saved to disk, replacing those of beta.
test_device_alpha.close()
stop_evsieve()
start_evsieve(TEST_DEVICE_PATH_IN_1)
assert(has_matching_capabilities(test_device_alpha))

# So far we've been testing capabilities. Now let's check if we can write events too without having evsieve
# destroy and recreate the device.
evsieve_output = evdev.InputDevice(TEST_DEVICE_PATH_OUT)
evsieve_output.grab()

test_device_alpha.open()
time.sleep(0.1)

def read_events(device: evdev.UInput) -> typing.List:
    return [
        (event.type, event.code, event.value)
        for event in device.read()
    ]

test_device_alpha.uinput.write(e.EV_KEY, e.KEY_C, 1)
test_device_alpha.uinput.syn()
time.sleep(0.1)
assert(read_events(evsieve_output) == [(e.EV_KEY, e.KEY_C, 1), (e.EV_SYN, 0, 0)])

# While we're at it, verify that the evsieve releases the pressed keys when the input device closes.
test_device_alpha.close()
assert(read_events(evsieve_output) == [(e.EV_KEY, e.KEY_C, 0), (e.EV_SYN, 0, 0)])

# Check if it goes right for another iteration without closing evsieve_output.
test_device_alpha.open()
time.sleep(0.1)
test_device_alpha.uinput.write(e.EV_KEY, e.KEY_A, 1)
test_device_alpha.uinput.syn()
test_device_alpha.uinput.write(e.EV_KEY, e.KEY_A, 0)
test_device_alpha.uinput.syn()
time.sleep(0.1)
assert(read_events(evsieve_output) == [
    (e.EV_KEY, e.KEY_A, 1), (e.EV_SYN, 0, 0),
    (e.EV_KEY, e.KEY_A, 0), (e.EV_SYN, 0, 0)
])

stop_evsieve()
test_device_alpha.close()

# Let's see if evsieve is able to recover from corrupted data in the persistence files.
# Note: evsieve does not "guarantee" any particular file structure other than that all of them
#       lie in $EVSIEVE_STATE_DIR. Some of following asserts are just to make sure that this
#       unittest works properly.
start_evsieve(test_device_alpha.path)
assert(has_matching_capabilities(test_device_alpha))
stop_evsieve()

EVSIEVE_DEVICE_CACHE_DIR = f"{EVSIEVE_STATE_DIR}/device-cache"
assert(os.path.isdir(EVSIEVE_DEVICE_CACHE_DIR))
device_cache_files = os.listdir(EVSIEVE_DEVICE_CACHE_DIR)
assert(len(device_cache_files) == 1)
device_cache_path = os.path.join(EVSIEVE_DEVICE_CACHE_DIR, device_cache_files[0])

with open(device_cache_path, "r+b") as file:
    file.write(b"corrupted")

start_evsieve(test_device_alpha.path)
assert(not has_matching_capabilities(test_device_alpha))  # Should mismatch because the cache was corrupted.
test_device_alpha.open()                                  # Tell evsieve again what the capabilities are.
time.sleep(0.1)
test_device_alpha.close()
stop_evsieve()

start_evsieve(test_device_alpha.path)
assert(has_matching_capabilities(test_device_alpha))      # Evsieve should've rebuilt the cache by now.
stop_evsieve()

# Now let's start working with a second device on another path and make sure evsieve can keep these
# two devices separate from each other.
test_device_gamma = VirtualInputDevice(TEST_DEVICE_PATH_IN_2, {e.EV_KEY: [e.KEY_Q]})
test_device_gamma.open()

start_evsieve(TEST_DEVICE_PATH_IN_2)
assert(not has_matching_capabilities(test_device_alpha))
assert(not has_matching_capabilities(test_device_beta))
assert(has_matching_capabilities(test_device_gamma))

stop_evsieve()
start_evsieve(TEST_DEVICE_PATH_IN_1)
assert(has_matching_capabilities(test_device_alpha))
assert(not has_matching_capabilities(test_device_beta))
assert(not has_matching_capabilities(test_device_gamma))

stop_evsieve()
test_device_gamma.close()
start_evsieve(TEST_DEVICE_PATH_IN_1)
assert(has_matching_capabilities(test_device_alpha))

stop_evsieve()
start_evsieve(TEST_DEVICE_PATH_IN_2)
assert(has_matching_capabilities(test_device_gamma))
stop_evsieve()

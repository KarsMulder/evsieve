#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-or-later

import evdev
import evdev.ecodes as e
import os
import subprocess as sp
import time

EVSIEVE_PATH = "target/debug/evsieve"

TEST_DEVICE_PATH_IN = "/dev/input/by-id/unittest-systemd-in"
TEST_DEVICE_PATH_OUT = "/dev/input/by-id/unittest-systemd-out"

capabilities = { e.EV_KEY: [e.KEY_A] }
input_device = evdev.UInput(capabilities)
if os.path.exists(TEST_DEVICE_PATH_IN) or os.path.islink(TEST_DEVICE_PATH_IN):
    raise Exception(f"Cannot carry out the unittest: required path is already occupied.")
os.symlink(input_device.device, TEST_DEVICE_PATH_IN)

# This test is mainly designed to verify whether evsieve works nicely with the "notify" service type.
sp.run([
    "systemd-run", "-Gu", "evsieve-unittest.service", "--service-type=notify", "--",
    EVSIEVE_PATH,
    "--input", TEST_DEVICE_PATH_IN, "grab=force",
    "--output", f"create-link={TEST_DEVICE_PATH_OUT}",
])

if not os.path.exists(TEST_DEVICE_PATH_OUT):
    raise Exception("Unit test failed: systemd-run has returned but the output device does not yet exist.")

output_device = evdev.InputDevice(TEST_DEVICE_PATH_OUT)
output_device.grab()

input_device.write(e.EV_KEY, e.KEY_A, 1)
input_device.syn()
input_device.write(e.EV_KEY, e.KEY_A, 0)
input_device.syn()

time.sleep(0.01)

events_read = []
for event in output_device.read():
    event = (event.type, event.code, event.value)
    events_read.append(event)
if events_read != [(1, 30, 1), (0, 0, 0), (1, 30, 0), (0, 0, 0)]:
    raise Exception("Unit test failed: invalid program output.")

output_device.close()
sp.run(["systemctl", "stop", "evsieve-unittest.service"])
input_device.close()
os.unlink(TEST_DEVICE_PATH_IN)

print("Unittest successful.")


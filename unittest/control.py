#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-or-later

import evdev
import evdev.ecodes as e
import os
import subprocess as sp
import time

EVSIEVE_PATH = "target/debug/evsieve"

TEST_DEVICE_PATH_IN = "/dev/input/by-id/unittest-control-in"
TEST_DEVICE_PATH_OUT = "/dev/input/by-id/unittest-control-out"
CONTROL_FIFO_PATH = "/tmp/evsieve-unittest-control-fifo"

if os.path.exists(CONTROL_FIFO_PATH):
    os.remove(CONTROL_FIFO_PATH)

capabilities = { e.EV_KEY: [e.KEY_A] }
input_device = evdev.UInput(capabilities)
if os.path.exists(TEST_DEVICE_PATH_IN) or os.path.islink(TEST_DEVICE_PATH_IN):
    raise Exception(f"Cannot carry out the unittest: required path is already occupied.")
os.symlink(input_device.device, TEST_DEVICE_PATH_IN)

sp.run([
    "systemd-run", "-Gu", "evsieve-unittest.service", "--service-type=notify", "--",
    EVSIEVE_PATH,
    "--input", TEST_DEVICE_PATH_IN, "grab=force",
    "--toggle", "", "key:b", "key:c",
    "--control-fifo", CONTROL_FIFO_PATH,
    "--output", f"create-link={TEST_DEVICE_PATH_OUT}",
])

if not os.path.exists(TEST_DEVICE_PATH_OUT):
    raise Exception("Unit test failed: systemd-run has returned but the output device does not yet exist.")

output_device = evdev.InputDevice(TEST_DEVICE_PATH_OUT)
output_device.grab()
assert(os.path.exists(CONTROL_FIFO_PATH))

def write_events(events):
    for event in events:
        input_device.write(*event)
        input_device.syn()

def expect_events(expected_events):
    events_read = []
    for event in output_device.read():
        event = (event.type, event.code, event.value)
        if event[0] != e.EV_SYN:
            events_read.append(event)
    if events_read != expected_events:
        raise Exception("Unit test failed: invalid program output.")

try:
    for i in range(5):
        with open(CONTROL_FIFO_PATH, "w") as file:
            file.write("toggle\n")
        time.sleep(0.01)

        write_events([(e.EV_KEY, e.KEY_A, 1), (e.EV_KEY, e.KEY_A, 0)])
        time.sleep(0.01)
        expect_events([(e.EV_KEY, e.KEY_C, 1), (e.EV_KEY, e.KEY_C, 0)])

        with open(CONTROL_FIFO_PATH, "w") as file:
            file.write("toggle\n")
        time.sleep(0.01)

        write_events([(e.EV_KEY, e.KEY_A, 1), (e.EV_KEY, e.KEY_A, 0)])
        time.sleep(0.01)
        expect_events([(e.EV_KEY, e.KEY_B, 1), (e.EV_KEY, e.KEY_B, 0)])

    success = True
except Exception as error:
    success = False
    print("Unittest failed: ", error)


output_device.close()
sp.run(["systemctl", "stop", "evsieve-unittest.service"])
input_device.close()
os.unlink(TEST_DEVICE_PATH_IN)

assert(not os.path.exists(CONTROL_FIFO_PATH))

if success:
    print("Unittest successful.")


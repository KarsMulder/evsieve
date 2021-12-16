#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-or-later

import evdev
import evdev.ecodes as e
from typing import *
import os
import subprocess as sp
import time

EVSIEVE_PROGRAM = ["target/debug/evsieve"]

sp.run(["systemctl", "reset-failed"])
def run_with_args(args):
    sp.run(["systemd-run", "--service-type=notify", "--collect", "--unit=evsieve-unittest.service"] + EVSIEVE_PROGRAM + args)

def terminate_subprocess():
    sp.run(["systemctl", "stop", "evsieve-unittest.service"])

# Part 1. Test whether evsieve reopens devices.

output_path = "/dev/input/by-id/evsieve-unittest-reopen-out"
symlink_chain = [
    "/dev/input/by-id/evsieve-unittest-reopen",
    "/tmp/evsieve-unittest/foo/link1",
    "/tmp/evsieve-unittest/bar/link2",
]

capabilities = {e.EV_KEY: [e.KEY_A]}
input_device = evdev.UInput(capabilities)

def create_links():
    global input_device

    current_path = input_device.device
    for link in symlink_chain:
        if os.path.exists(link) or os.path.islink(link):
            os.unlink(link)
        os.makedirs(os.path.dirname(link), exist_ok=True)
        os.symlink(current_path, link)
        current_path = link

create_links()

run_with_args([
    "--input", symlink_chain[-1], "grab=force", "persist=reopen",
    "--output", f"create-link={output_path}"
])

output_device = evdev.InputDevice(output_path)
output_device.grab()

def test_send_events():
    global input_device
    global output_device

    expected_events = [(e.EV_KEY, e.KEY_A, 1), (e.EV_SYN, 0, 0), (e.EV_KEY, e.KEY_A, 0), (e.EV_SYN, 0, 0)]

    for event in expected_events:
        input_device.write(event[0], event[1], event[2])
    time.sleep(0.01)

    received_events = 0
    for expected_event, real_event in zip(expected_events, output_device.read()):
        event = (real_event.type, real_event.code, real_event.value)
        assert(expected_event == event)
        received_events += 1
    assert(received_events == len(expected_events))

test_send_events()

# Destroy the device and then recreate it.
input_device.close()
for link in symlink_chain:
    os.unlink(link)
time.sleep(0.1)

input_device = evdev.UInput(capabilities)
create_links()
time.sleep(0.1)

# Test whether evsieve has picked up the new device.
test_send_events()

# This time only destroy the last link in the chain.
input_device.close()
os.unlink(symlink_chain[0])
time.sleep(0.1)

input_device = evdev.UInput(capabilities)
os.symlink(input_device.device, symlink_chain[0])
time.sleep(0.1)

test_send_events()

print("Unittest part 1 successful.")

terminate_subprocess()
input_device.close()
output_device.close()
time.sleep(0.2)

# Part 2: testing whether evsieve exits when all devices are closed with persist=none.
output_path = "/dev/input/by-id/evsieve-unittest-reopen-out"
assert(not os.path.islink(output_path))

symlink_chain = [
    "/tmp/evsieve-unittest/link1",
    "/tmp/evsieve-unittest/link2",
]

capabilities_A = {e.EV_KEY: [e.KEY_A]}
capabilities_B = {e.EV_KEY: [e.KEY_B]}
capabilities_C = {e.EV_KEY: [e.KEY_C]}
capabilities_AB = {e.EV_KEY: [e.KEY_A, e.KEY_B]}

input_device_1 = None
input_device_2 = None
input_device_1_path = "/tmp/evsieve-unittest/link1"
input_device_2_path = "/tmp/evsieve-unittest/link2"

def link_to_device(device, link):
    if os.path.exists(link) or os.path.islink(link):
        os.unlink(link)
    os.makedirs(os.path.dirname(link), exist_ok=True)
    os.symlink(device.device, link)

def create_input_devices(capabilities):
    global input_device_1
    global input_device_2
    # The links need to be deleted before output devices are created, otherwise a race condition happens:
    # Upon creating input_device_1, evsieve may create a virtual output device that just happens to be
    # at the location that input_device_2_path points to before input_device_2 is created.
    for link in [input_device_1_path, input_device_2_path]:
        if os.path.exists(link) or os.path.islink(link):
            os.unlink(link)
    input_device_1 = evdev.UInput(capabilities)
    input_device_2 = evdev.UInput(capabilities)
    link_to_device(input_device_1, input_device_1_path)
    link_to_device(input_device_2, input_device_2_path)

create_input_devices(capabilities_A)


run_with_args([
    "--input", input_device_1_path, "persist=none", "grab=force",
    "--input", input_device_2_path, "persist=none", "grab=force",
    "--output", f"create-link={output_path}"
])

time.sleep(0.2)
assert(os.path.exists(output_path))

input_device_1.close()
time.sleep(0.2)
assert(os.path.exists(output_path))

# Make sure evsieve exits when all input devices disappear.
input_device_2.close()
time.sleep(0.2)
assert(not os.path.exists(output_path) and not os.path.islink(output_path))

print("Unittest part 2 successful.")

# Part 3: test whether evsieve handles devices with changing capabilities properly.

create_input_devices(capabilities_AB)
run_with_args([
    "--input", input_device_1_path, "persist=reopen", "grab=force",
    "--input", input_device_2_path, "persist=reopen", "grab=force",
    "--output", f"create-link={output_path}"
])
time.sleep(0.1)
output_device = evdev.InputDevice(output_path)
output_device.grab()

def test_events(input_device, output_device, events):
    for event in events:
        input_device.write(event[0], event[1], event[2])
    time.sleep(0.01)

    received_events = 0
    for expected_event, real_event in zip(events, output_device.read()):
        event = (real_event.type, real_event.code, real_event.value)
        assert(expected_event == event)
        received_events += 1
    assert(received_events == len(events))

test_events(input_device_2, output_device,
    [(e.EV_KEY, e.KEY_A, 1), (e.EV_SYN, 0, 0), (e.EV_KEY, e.KEY_A, 0), (e.EV_SYN, 0, 0)]
)

# Upon closing an input device, all pressed keys should be released.
input_device_1.write(e.EV_KEY, e.KEY_A, 1)
input_device_1.write(e.EV_SYN, 0, 0)
input_device_1.close()

time.sleep(0.05)
expected_output = [(e.EV_KEY, e.KEY_A, 1), (e.EV_SYN, 0, 0), (e.EV_KEY, e.KEY_A, 0), (e.EV_SYN, 0, 0)]
real_output = [(event.type, event.code, event.value) for event in output_device.read()]
assert(expected_output == real_output)

# Upon recreating a device with the same or lesser capabilities, the output device should not be recreated.
os.unlink(input_device_1_path)
input_device_1 = evdev.UInput(capabilities_A)
link_to_device(input_device_1, input_device_1_path)

time.sleep(0.1)
test_events(input_device_1, output_device,
    [(e.EV_KEY, e.KEY_A, 1), (e.EV_SYN, 0, 0), (e.EV_KEY, e.KEY_A, 0), (e.EV_SYN, 0, 0)]
)
test_events(input_device_2, output_device,
    [(e.EV_KEY, e.KEY_B, 1), (e.EV_SYN, 0, 0), (e.EV_KEY, e.KEY_B, 0), (e.EV_SYN, 0, 0)]
)

# Upon recreating a device with more capabilities, the output device should be recreated.
input_device_1.close()
os.unlink(input_device_1_path)
input_device_1 = evdev.UInput(capabilities_C)
link_to_device(input_device_1, input_device_1_path)
time.sleep(0.1)

output_device.close()
output_device = evdev.InputDevice(output_path)
output_device.grab()

test_events(input_device_1, output_device,
    [(e.EV_KEY, e.KEY_C, 1), (e.EV_SYN, 0, 0), (e.EV_KEY, e.KEY_C, 0), (e.EV_SYN, 0, 0)]
)
test_events(input_device_2, output_device,
    [(e.EV_KEY, e.KEY_B, 1), (e.EV_SYN, 0, 0), (e.EV_KEY, e.KEY_B, 0), (e.EV_SYN, 0, 0)]
)

input_device_1.close()
input_device_2.close()
output_device.close()
terminate_subprocess()
os.unlink(input_device_1_path)
os.unlink(input_device_2_path)

print("Unittest part 3 successful.")

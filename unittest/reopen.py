#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-or-later

import evdev
import evdev.ecodes as e
from typing import *
import os
import subprocess as sp
import time

EVSIEVE_PROGRAM = ["target/debug/evsieve"]

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

subprocess = sp.Popen(EVSIEVE_PROGRAM + [
    "--input", symlink_chain[-1], "grab=force",
    "--output", f"create-link={output_path}"
])

time.sleep(0.5)

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

print("Unittest successful.")

subprocess.kill()
input_device.close()
output_device.close()

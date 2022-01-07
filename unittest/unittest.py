#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-or-later

import evdev
import evdev.ecodes as e
from typing import *
import os
import subprocess as sp
import time

EVSIEVE_PROGRAM = ["target/debug/evsieve"]

def run_unittest(
    arguments: List[str],
    input: Dict[str, List[Tuple[int, int, int]]],
    output: Dict[str, List[Tuple[int, int, int]]],
    auto_syn = True,
    expected_output = None,
):
    # Create virtual input devices.
    input_devices = dict()
    for path, events in input.items():
        type_capabilities = set([type for (type, _, _) in events])
        capabilities = {
            type: [
                code
                for (_type, code, _) in events
                if _type == type
            ]
            for type in type_capabilities
        }
        input_device = evdev.UInput(capabilities)
        if os.path.exists(path) or os.path.islink(path):
            raise Exception(f"Cannot carry out the unittest: required path {path} is already occupied.")
        os.symlink(input_device.device, path)
        input_devices[input_device] = events

    # Run the actual program.
    process = sp.Popen(EVSIEVE_PROGRAM + arguments, stdout=sp.PIPE)
    # Give the process some time to create the output devices.
    time.sleep(0.2)

    try:
        # Open the output devices.
        output_devices = []
        for path, events in output.items():
            output_device = evdev.InputDevice(path)
            output_device.grab()
            output_devices.append((output_device, events))

        # Send the input events.
        for device, events in input_devices.items():
            for event in events:
                device.write(*event)
                if auto_syn:
                    device.syn()
                time.sleep(0.01)

        # Check whether the output devices have the expected events.
        time.sleep(0.05)
        for device, events in output_devices:
            for event in device.read():
                if auto_syn and event.type == e.EV_SYN:
                    continue
                expected_event = events.pop(0)
                event = (event.type, event.code, event.value)
                if event != expected_event:
                    raise Exception(f"Unit test failed. Expected event {expected_event}, encountered {event}")
            if len(events) > 0:
                raise Exception(f"Unit test failed. Expected events {events}, but the output device closed.")

    finally:
        # Clean up.
        for device, _ in output_devices:
            device.close()
        if expected_output is not None:
            time.sleep(0.2)
        process.terminate()
        for device in input_devices.keys():
            device.close()
        for path in input.keys():
            os.unlink(path)
        if expected_output is not None:
            output = process.stdout.read().decode("utf8")
            if output != expected_output:
                raise Exception(f"Unittest failed. Expected the following output:\n{expected_output}\nGot:\n{output}")


def unittest_mirror():
    run_unittest(
        ["--input", "/dev/input/by-id/unittest-mirror-in", "grab=force",
        "--output", "create-link=/dev/input/by-id/unittest-mirror-out", "repeat=passive"],
        {
            "/dev/input/by-id/unittest-mirror-in": [
                (e.EV_KEY, e.KEY_A, 1),
                (e.EV_KEY, e.KEY_A, 2),
                (e.EV_KEY, e.KEY_A, 2),
                (e.EV_KEY, e.KEY_A, 0),
                (e.EV_KEY, e.KEY_B, 1),
                (e.EV_KEY, e.KEY_C, 1),
                (e.EV_KEY, e.KEY_B, 0),
                (e.EV_KEY, e.KEY_C, 0),
            ],
        },
        {
            "/dev/input/by-id/unittest-mirror-out": [
                (e.EV_KEY, e.KEY_A, 1),
                (e.EV_KEY, e.KEY_A, 2),
                (e.EV_KEY, e.KEY_A, 2),
                (e.EV_KEY, e.KEY_A, 0),
                (e.EV_KEY, e.KEY_B, 1),
                (e.EV_KEY, e.KEY_C, 1),
                (e.EV_KEY, e.KEY_B, 0),
                (e.EV_KEY, e.KEY_C, 0),
            ],
        },
    )

def unittest_capslock():
    run_unittest(
        ["--input", "/dev/input/by-id/unittest-capslock-in", "grab=force",
        "--map", "key:capslock", "key:backspace",
        "--output", "create-link=/dev/input/by-id/unittest-capslock-out", "repeat=passive"],
        {
            "/dev/input/by-id/unittest-capslock-in": [
                (e.EV_KEY, e.KEY_CAPSLOCK, 1),
                (e.EV_KEY, e.KEY_CAPSLOCK, 2),
                (e.EV_KEY, e.KEY_CAPSLOCK, 0),
            ],
        },
        {
            "/dev/input/by-id/unittest-capslock-out": [
                (e.EV_KEY, e.KEY_BACKSPACE, 1),
                (e.EV_KEY, e.KEY_BACKSPACE, 2),
                (e.EV_KEY, e.KEY_BACKSPACE, 0),
            ],
        },
    )

def unittest_doublectrl():
    run_unittest(
        ["--input", "/dev/input/by-id/unittest-doublectrl-in", "grab=force",
        "--map", "key:scrolllock", "key:leftctrl", "key:rightctrl",
        "--output", "create-link=/dev/input/by-id/unittest-doublectrl-out", "repeat=passive"],
        {
            "/dev/input/by-id/unittest-doublectrl-in": [
                (e.EV_KEY, e.KEY_LEFTCTRL, 1),
                (e.EV_KEY, e.KEY_LEFTCTRL, 0),
                (e.EV_KEY, e.KEY_SCROLLLOCK, 1),
                (e.EV_KEY, e.KEY_SCROLLLOCK, 2),
                (e.EV_KEY, e.KEY_SCROLLLOCK, 0),
            ],
        },
        {
            "/dev/input/by-id/unittest-doublectrl-out": [
                (e.EV_KEY, e.KEY_LEFTCTRL, 1),
                (e.EV_KEY, e.KEY_LEFTCTRL, 0),
                (e.EV_KEY, e.KEY_LEFTCTRL, 1),
                (e.EV_KEY, e.KEY_RIGHTCTRL, 1),
                (e.EV_KEY, e.KEY_LEFTCTRL, 2),
                (e.EV_KEY, e.KEY_RIGHTCTRL, 2),
                (e.EV_KEY, e.KEY_LEFTCTRL, 0),
                (e.EV_KEY, e.KEY_RIGHTCTRL, 0),
            ],
        },
    )

def unittest_filterbyoutput():
    run_unittest(
        ["--input", "/dev/input/by-id/unittest-filterbyoutput-in", "grab=force",
        "--output", "key:a", "create-link=/dev/input/by-id/unittest-filterbyoutput-out-1",
        "--output", "create-link=/dev/input/by-id/unittest-filterbyoutput-out-2"],
        {
            "/dev/input/by-id/unittest-filterbyoutput-in": [
                (e.EV_KEY, e.KEY_A, 1),
                (e.EV_KEY, e.KEY_B, 1),
                (e.EV_KEY, e.KEY_B, 0),
                (e.EV_KEY, e.KEY_A, 0),
            ],
        },
        {
            "/dev/input/by-id/unittest-filterbyoutput-out-1": [
                (e.EV_KEY, e.KEY_A, 1),
                (e.EV_KEY, e.KEY_A, 0),
            ],
            "/dev/input/by-id/unittest-filterbyoutput-out-2": [
                (e.EV_KEY, e.KEY_B, 1),
                (e.EV_KEY, e.KEY_B, 0),
            ],
        },
    )

def unittest_domain():
    run_unittest(
        ["--input", "/dev/input/by-id/unittest-domain-in", "domain=in", "grab=force",
        "--map", "key:a", "key:a@out-1",
        "--map", "@in", "@out-1", "@out-2",
        "--output", "@out-1", "create-link=/dev/input/by-id/unittest-domain-out-1",
        "--output", "@out-2", "create-link=/dev/input/by-id/unittest-domain-out-2"],
        {
            "/dev/input/by-id/unittest-domain-in": [
                (e.EV_KEY, e.KEY_A, 1),
                (e.EV_KEY, e.KEY_B, 1),
                (e.EV_KEY, e.KEY_B, 0),
                (e.EV_KEY, e.KEY_A, 0),
            ],
        },
        {
            "/dev/input/by-id/unittest-domain-out-1": [
                (e.EV_KEY, e.KEY_A, 1),
                (e.EV_KEY, e.KEY_B, 1),
                (e.EV_KEY, e.KEY_B, 0),
                (e.EV_KEY, e.KEY_A, 0),
            ],
            "/dev/input/by-id/unittest-domain-out-2": [
                (e.EV_KEY, e.KEY_B, 1),
                (e.EV_KEY, e.KEY_B, 0),
            ],
        },
    )

def unittest_kbmousemap():
    args = ["--input", "/dev/input/by-id/unittest-kbmousemap-kb-in", "grab=force",
        "--input", "/dev/input/by-id/unittest-kbmousemap-mouse-in", "grab=force", "domain=mouse",
        "--map", "key:left:1~2",      "rel:x:-20@mouse",
        "--map", "key:right:1~2",     "rel:x:20@mouse",
        "--map", "key:up:1~2",        "rel:y:-20@mouse",
        "--map", "key:down:1~2",      "rel:y:20@mouse",
        "--map", "key:enter:0~1",     "btn:left@mouse",
        "--map", "key:backslash:0~1", "btn:right@mouse",
        "--output", "@mouse", "create-link=/dev/input/by-id/unittest-kbmousemap-mouse-out", "repeat=passive"]
    
    run_unittest(
        args,
        {
            "/dev/input/by-id/unittest-kbmousemap-kb-in": [
                (e.EV_KEY, e.KEY_LEFT, 1),
                (e.EV_KEY, e.KEY_LEFT, 2),
                (e.EV_KEY, e.KEY_LEFT, 2),
                (e.EV_KEY, e.KEY_LEFT, 0),
                (e.EV_KEY, e.BTN_LEFT, 1),
                (e.EV_KEY, e.BTN_LEFT, 0),
                (e.EV_KEY, e.KEY_BACKSLASH, 1),
                (e.EV_KEY, e.KEY_BACKSLASH, 2),
                (e.EV_KEY, e.KEY_BACKSLASH, 0),
            ],
            "/dev/input/by-id/unittest-kbmousemap-mouse-in": [],
        },
        {
            "/dev/input/by-id/unittest-kbmousemap-mouse-out": [
                (e.EV_REL, e.REL_X, -20),
                (e.EV_REL, e.REL_X, -20),
                (e.EV_REL, e.REL_X, -20),
                (e.EV_KEY, e.BTN_RIGHT, 1),
                (e.EV_KEY, e.BTN_RIGHT, 0),
            ],
        }
    )
    
    run_unittest(
        args,
        {
            "/dev/input/by-id/unittest-kbmousemap-kb-in": [],
            "/dev/input/by-id/unittest-kbmousemap-mouse-in": [
                (e.EV_REL, e.REL_Y, 10),
                (e.EV_KEY, e.BTN_LEFT, 1),
                (e.EV_KEY, e.BTN_LEFT, 0),
            ],
        },
        {
            "/dev/input/by-id/unittest-kbmousemap-mouse-out": [
                (e.EV_REL, e.REL_Y, 10),
                (e.EV_KEY, e.BTN_LEFT, 1),
                (e.EV_KEY, e.BTN_LEFT, 0),
            ],
        }
    )
    
def unittest_execshell():
    run_unittest(
        ["--input", "/dev/input/by-id/unittest-execshell-in", "grab=force",
        "--hook", "key:t", "exec-shell=echo trigger1",
        "--hook", "key:a", "key:b", "exec-shell=echo trigger2",
        "--hook", "key:q", "key:w", "exec-shell=echo trigger3",
        "--hook", "key:z", "key:x", "key:c", "exec-shell=echo trigger4",
        "--hook", "key:e", "key:i", "key:o", "exec-shell=echo trigger5",
        "--hook", "key:n:2", "exec-shell=echo trigger6"],
        {
            "/dev/input/by-id/unittest-execshell-in": [
                (e.EV_KEY, e.KEY_T, 1),
                (e.EV_KEY, e.KEY_T, 2),
                (e.EV_KEY, e.KEY_T, 0),
                (e.EV_KEY, e.KEY_T, 1),
                (e.EV_KEY, e.KEY_T, 0),

                (e.EV_KEY, e.KEY_B, 1),
                (e.EV_KEY, e.KEY_B, 2),
                (e.EV_KEY, e.KEY_A, 1),
                (e.EV_KEY, e.KEY_A, 2),
                (e.EV_KEY, e.KEY_A, 0),
                (e.EV_KEY, e.KEY_B, 0),

                (e.EV_KEY, e.KEY_Q, 1),
                (e.EV_KEY, e.KEY_W, 1),
                (e.EV_KEY, e.KEY_Q, 2),
                (e.EV_KEY, e.KEY_W, 2),
                (e.EV_KEY, e.KEY_Q, 0),
                (e.EV_KEY, e.KEY_W, 0),
                (e.EV_KEY, e.KEY_Q, 1),
                (e.EV_KEY, e.KEY_Q, 0),

                (e.EV_KEY, e.KEY_X, 1),
                (e.EV_KEY, e.KEY_C, 1), 
                (e.EV_KEY, e.KEY_Z, 1),
                (e.EV_KEY, e.KEY_X, 0),
                (e.EV_KEY, e.KEY_C, 0),
                (e.EV_KEY, e.KEY_Z, 0),
                
                # Should not trigger: O is not pressed.
                (e.EV_KEY, e.KEY_O, 1),
                (e.EV_KEY, e.KEY_O, 0),
                (e.EV_KEY, e.KEY_I, 1),
                (e.EV_KEY, e.KEY_E, 1),
                (e.EV_KEY, e.KEY_E, 0),
                (e.EV_KEY, e.KEY_I, 0),

                (e.EV_KEY, e.KEY_N, 1),
                (e.EV_KEY, e.KEY_N, 0),
                (e.EV_KEY, e.KEY_N, 1),
                (e.EV_KEY, e.KEY_N, 2),
                (e.EV_KEY, e.KEY_N, 2),
                (e.EV_KEY, e.KEY_N, 2),
                (e.EV_KEY, e.KEY_N, 0),
                (e.EV_KEY, e.KEY_N, 1),
                (e.EV_KEY, e.KEY_N, 2),
                (e.EV_KEY, e.KEY_N, 0),
            ],
        },
        {},
        expected_output = "trigger1\ntrigger1\ntrigger2\ntrigger3\ntrigger4\ntrigger6\ntrigger6\n",
    )

def unittest_toggle():
    run_unittest(
        ["--input", "/dev/input/by-id/unittest-toggle-in", "domain=in", "grab=force",
        "--toggle", "@in", "@out-1", "@out-2", "id=output-toggle",
        "--toggle", "key:a", "key:b", "key:c", "id=a-toggle",
        "--toggle", "key:b", "key:e", "key:f",
        "--hook", "key:z", "toggle",
        "--hook", "key:x", "toggle=a-toggle",
        "--hook", "key:v", "toggle=a-toggle:1", "toggle",
        "--hook", "key:q", "toggle=a-toggle:2",
        "--output", "@out-1", "create-link=/dev/input/by-id/unittest-toggle-out-1",
        "--output", "@out-2", "create-link=/dev/input/by-id/unittest-toggle-out-2"],
    {
        "/dev/input/by-id/unittest-toggle-in": [
            (e.EV_KEY, e.KEY_A, 1),
            (e.EV_KEY, e.KEY_A, 0),
            (e.EV_KEY, e.KEY_Z, 1),
            (e.EV_KEY, e.KEY_Z, 0),
            (e.EV_KEY, e.KEY_A, 1),
            (e.EV_KEY, e.KEY_A, 0),

            (e.EV_KEY, e.KEY_A, 1),
            (e.EV_KEY, e.KEY_X, 1),
            (e.EV_KEY, e.KEY_X, 0),
            (e.EV_KEY, e.KEY_A, 0),
            (e.EV_KEY, e.KEY_A, 1),
            (e.EV_KEY, e.KEY_A, 0),
            
            (e.EV_KEY, e.KEY_V, 1),
            (e.EV_KEY, e.KEY_V, 0),
            (e.EV_KEY, e.KEY_N, 1),
            (e.EV_KEY, e.KEY_N, 0),
            (e.EV_KEY, e.KEY_A, 1),
            (e.EV_KEY, e.KEY_A, 0),

            (e.EV_KEY, e.KEY_Q, 1),
            (e.EV_KEY, e.KEY_Q, 0),
            (e.EV_KEY, e.KEY_A, 1),
            (e.EV_KEY, e.KEY_A, 0),
        ],
    },
    {
        "/dev/input/by-id/unittest-toggle-out-1": [
            (e.EV_KEY, e.KEY_E, 1),
            (e.EV_KEY, e.KEY_E, 0),
            (e.EV_KEY, e.KEY_Z, 1),
            (e.EV_KEY, e.KEY_Z, 0),

            (e.EV_KEY, e.KEY_N, 1),
            (e.EV_KEY, e.KEY_N, 0),
            (e.EV_KEY, e.KEY_E, 1),
            (e.EV_KEY, e.KEY_E, 0),

            (e.EV_KEY, e.KEY_Q, 1),
            (e.EV_KEY, e.KEY_Q, 0),
            (e.EV_KEY, e.KEY_C, 1),
            (e.EV_KEY, e.KEY_C, 0),
        ],
        "/dev/input/by-id/unittest-toggle-out-2": [
            (e.EV_KEY, e.KEY_C, 1),
            (e.EV_KEY, e.KEY_C, 0),

            (e.EV_KEY, e.KEY_C, 1),
            (e.EV_KEY, e.KEY_X, 1),
            (e.EV_KEY, e.KEY_X, 0),
            (e.EV_KEY, e.KEY_C, 0),
            (e.EV_KEY, e.KEY_F, 1),
            (e.EV_KEY, e.KEY_F, 0),

            (e.EV_KEY, e.KEY_V, 1),
            (e.EV_KEY, e.KEY_V, 0),
        ],
    })

def unittest_yield():
    run_unittest(
        ["--input", "/dev/input/by-id/unittest-yield-in", "grab=force",
        "--map", "key:a", "key:b",
        "--map", "key:b", "key:c",
        "--map", "key:d", "key:e", "yield",
        "--map", "key:e", "key:f",
        "--copy", "key:g", "key:h", "yield",
        "--copy", "key:h", "key:i",
        "--block", "key:g",
        "--output", "create-link=/dev/input/by-id/unittest-yield-out"],
        {
            "/dev/input/by-id/unittest-yield-in": [
                (e.EV_KEY, e.KEY_A, 1),
                (e.EV_KEY, e.KEY_A, 0),
                (e.EV_KEY, e.KEY_D, 1),
                (e.EV_KEY, e.KEY_D, 0),
                (e.EV_KEY, e.KEY_G, 1),
                (e.EV_KEY, e.KEY_G, 0),
            ],
        },
        {
            "/dev/input/by-id/unittest-yield-out": [
                (e.EV_KEY, e.KEY_C, 1),
                (e.EV_KEY, e.KEY_C, 0),
                (e.EV_KEY, e.KEY_E, 1),
                (e.EV_KEY, e.KEY_E, 0),
                (e.EV_KEY, e.KEY_H, 1),
                (e.EV_KEY, e.KEY_H, 0),
            ],
        },
    )

def unittest_order():
    run_unittest(
        ["--input", "/dev/input/by-id/unittest-order-in", "grab=force",
        "--copy", "key:a", "key:d", "key:f",
        "--map", "key:d", "key:d", "key:e", "yield",
        "--map", "key:a", "key:b", "key:c",
        "--copy", "key:f", "key:g",
        "--output", "create-link=/dev/input/by-id/unittest-order-out"],
        {
            "/dev/input/by-id/unittest-order-in": [
                (e.EV_KEY, e.KEY_A, 1),
                (e.EV_KEY, e.KEY_A, 0),
            ],
        },
        {
            "/dev/input/by-id/unittest-order-out": [
                # The order of the following events is important: it's the very thing
                # this unittest is intended to test.
                (e.EV_KEY, e.KEY_B, 1),
                (e.EV_KEY, e.KEY_C, 1),
                (e.EV_KEY, e.KEY_D, 1),
                (e.EV_KEY, e.KEY_E, 1),
                (e.EV_KEY, e.KEY_F, 1),
                (e.EV_KEY, e.KEY_G, 1),
                (e.EV_KEY, e.KEY_B, 0),
                (e.EV_KEY, e.KEY_C, 0),
                (e.EV_KEY, e.KEY_D, 0),
                (e.EV_KEY, e.KEY_E, 0),
                (e.EV_KEY, e.KEY_F, 0),
                (e.EV_KEY, e.KEY_G, 0),
            ],
        },
    )

def unittest_namespace():
    run_unittest(
        ["--hook", "key:a", "exec-shell=echo foo",
        "--copy", "key:a", "key:b",
        "--input", "/dev/input/by-id/unittest-namespace-in-1", "grab=force",
        "--copy", "key:a", "key:c",
        "--input", "/dev/input/by-id/unittest-namespace-in-2", "grab=force",
        "--copy", "key:a", "key:d",
        "--hook", "key:a", "exec-shell=echo bar",
        "--output", "create-link=/dev/input/by-id/unittest-namespace-out-1",
        "--input", "/dev/input/by-id/unittest-namespace-in-3", "grab=force",
        "--copy", "key:a", "key:e",
        "--output", "create-link=/dev/input/by-id/unittest-namespace-out-2",
        "--hook", "key:a", "exec-shell=echo baz",],
        {
            "/dev/input/by-id/unittest-namespace-in-1": [
                (e.EV_KEY, e.KEY_Q, 1),
                (e.EV_KEY, e.KEY_Q, 0),
            ],
            "/dev/input/by-id/unittest-namespace-in-2": [
                (e.EV_KEY, e.KEY_A, 1),
                (e.EV_KEY, e.KEY_A, 0),
            ],
            "/dev/input/by-id/unittest-namespace-in-3": [
                (e.EV_KEY, e.KEY_B, 1),
                (e.EV_KEY, e.KEY_B, 0),
            ],
        },
        {
            "/dev/input/by-id/unittest-namespace-out-1": [
                (e.EV_KEY, e.KEY_Q, 1),
                (e.EV_KEY, e.KEY_Q, 0),
                (e.EV_KEY, e.KEY_A, 1),
                (e.EV_KEY, e.KEY_D, 1),
                (e.EV_KEY, e.KEY_A, 0),
                (e.EV_KEY, e.KEY_D, 0),
            ],
            "/dev/input/by-id/unittest-namespace-out-2": [
                (e.EV_KEY, e.KEY_B, 1),
                (e.EV_KEY, e.KEY_B, 0),
            ],
        },
        expected_output="bar\n",
    )

def unittest_consistency():
    run_unittest(
        ["--input", "/dev/input/by-id/unittest-consistency-in", "domain=foo", "grab=force",
        "--map", "key:x", "key:a@bar",
        "--toggle", "key:a", "key:b", "key:c",
        "--hook", "key:z", "toggle",
        "--output", "@foo", "create-link=/dev/input/by-id/unittest-consistency-out-1",
        "--output", "@bar", "create-link=/dev/input/by-id/unittest-consistency-out-2"],
        {
            "/dev/input/by-id/unittest-consistency-in": [
                (e.EV_KEY, e.KEY_A, 1),
                (e.EV_KEY, e.KEY_X, 1),
                (e.EV_KEY, e.KEY_Z, 1),
                (e.EV_KEY, e.KEY_Z, 0),
                (e.EV_KEY, e.KEY_X, 0),
                (e.EV_KEY, e.KEY_A, 0),
            ],
        },
        {
            "/dev/input/by-id/unittest-consistency-out-1": [
                (e.EV_KEY, e.KEY_B, 1),
                (e.EV_KEY, e.KEY_Z, 1),
                (e.EV_KEY, e.KEY_Z, 0),
                (e.EV_KEY, e.KEY_B, 0),
            ],
            "/dev/input/by-id/unittest-consistency-out-2": [
                (e.EV_KEY, e.KEY_B, 1),
                (e.EV_KEY, e.KEY_B, 0),
            ],
        },
    )

def unittest_type():
    run_unittest(
        ["--input", "/dev/input/by-id/unittest-type-in-1", "domain=in1", "grab=force",
        "--input", "/dev/input/by-id/unittest-type-in-2", "domain=in2", "grab=force",
        "--map", "key@in1", "key:a",
        "--map", "btn", "btn:left",
        "--map", "key::2", "rel:y:3",
        "--output", "btn", "@in1", "rel", "create-link=/dev/input/by-id/unittest-type-out-1",
        "--output", "create-link=/dev/input/by-id/unittest-type-out-2"],
        {
            "/dev/input/by-id/unittest-type-in-1": [
                (e.EV_KEY, e.KEY_B, 1),
                (e.EV_KEY, e.KEY_B, 2),
                (e.EV_KEY, e.KEY_B, 0),
                (e.EV_ABS, e.ABS_X, 1),
                (e.EV_ABS, e.ABS_X, 0),
            ],
            "/dev/input/by-id/unittest-type-in-2": [
                (e.EV_KEY, e.KEY_B, 1),
                (e.EV_KEY, e.KEY_B, 0),
                (e.EV_KEY, e.BTN_RIGHT, 1),
                (e.EV_KEY, e.BTN_RIGHT, 0),
            ],
        },
        {
            "/dev/input/by-id/unittest-type-out-1": [
                (e.EV_KEY, e.KEY_A, 1),
                (e.EV_REL, e.REL_Y, 3),
                (e.EV_KEY, e.KEY_A, 0),
                (e.EV_ABS, e.ABS_X, 1),
                (e.EV_ABS, e.ABS_X, 0),
                (e.EV_KEY, e.BTN_LEFT, 1),
                (e.EV_KEY, e.BTN_LEFT, 0),
            ],
            "/dev/input/by-id/unittest-type-out-2": [
                (e.EV_KEY, e.KEY_B, 1),
                (e.EV_KEY, e.KEY_B, 0),
            ],
        },
    )

def unittest_merge():
    run_unittest(
        ["--input", "/dev/input/by-id/unittest-merge-in-1", "domain=in1", "grab=force",
        "--map", "key:b", "key:a",
        "--map", "key:y", "key:x",
        "--map", "key:t:1", "key:a:0",
        "--block", "key:t",
        "--merge", "key:a",
        "--output", "create-link=/dev/input/by-id/unittest-merge-out-1"],
        {
            "/dev/input/by-id/unittest-merge-in-1": [
                (e.EV_KEY, e.KEY_A, 1),
                (e.EV_KEY, e.KEY_B, 1),
                (e.EV_KEY, e.KEY_C, 1),
                (e.EV_KEY, e.KEY_A, 0),
                (e.EV_KEY, e.KEY_C, 0),
                (e.EV_KEY, e.KEY_B, 0),

                (e.EV_KEY, e.KEY_X, 1),
                (e.EV_KEY, e.KEY_Y, 1),
                (e.EV_KEY, e.KEY_Z, 1),
                (e.EV_KEY, e.KEY_X, 0),
                (e.EV_KEY, e.KEY_Z, 0),
                (e.EV_KEY, e.KEY_Y, 0),

                (e.EV_KEY, e.KEY_T, 1),
                (e.EV_KEY, e.KEY_T, 0),
                (e.EV_KEY, e.KEY_A, 1),
                (e.EV_KEY, e.KEY_C, 1),
                (e.EV_KEY, e.KEY_A, 0),
                (e.EV_KEY, e.KEY_C, 0),

                (e.EV_ABS, e.ABS_X, 10),
                (e.EV_ABS, e.ABS_X, 0),
            ],
        },
        {
            "/dev/input/by-id/unittest-merge-out-1": [
                (e.EV_KEY, e.KEY_A, 1),
                (e.EV_KEY, e.KEY_C, 1),
                (e.EV_KEY, e.KEY_C, 0),
                (e.EV_KEY, e.KEY_A, 0),

                (e.EV_KEY, e.KEY_X, 1),
                (e.EV_KEY, e.KEY_Z, 1),
                (e.EV_KEY, e.KEY_X, 0),
                (e.EV_KEY, e.KEY_Z, 0),

                (e.EV_KEY, e.KEY_A, 1),
                (e.EV_KEY, e.KEY_C, 1),
                (e.EV_KEY, e.KEY_A, 0),
                (e.EV_KEY, e.KEY_C, 0),

                (e.EV_ABS, e.ABS_X, 10),
                (e.EV_ABS, e.ABS_X, 0),
            ],
        },
    )

def unittest_delta():
    run_unittest(
        ["--input", "/dev/input/by-id/unittest-delta-in", "grab=force",
        "--map", "abs:x", "rel:x:0.5d",
        "--output", "create-link=/dev/input/by-id/unittest-delta-out"],
        {
            "/dev/input/by-id/unittest-delta-in": [
                # Test evsieve's resistance to rounding errors: the first movement should be
                # rounded down, the second rounded up.
                (e.EV_ABS, e.ABS_X, 7),
                (e.EV_ABS, e.ABS_X, 10),
                (e.EV_ABS, e.ABS_X, 0),
            ],
        },
        {
            "/dev/input/by-id/unittest-delta-out": [
                (e.EV_REL, e.REL_X, 3),
                (e.EV_REL, e.REL_X, 2),
                (e.EV_REL, e.REL_X, -5),
            ],
        },
    )


unittest_mirror()
unittest_capslock()
unittest_doublectrl()
unittest_filterbyoutput()
unittest_domain()
unittest_kbmousemap()
unittest_execshell()
unittest_toggle()
unittest_yield()
unittest_order()
unittest_namespace()
unittest_consistency()
unittest_type()
unittest_merge()
unittest_delta()

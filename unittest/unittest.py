#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-or-later

import evdev
import evdev.ecodes as e
from typing import *
import os
import subprocess as sp
import time

EVSIEVE_PROGRAM = ["target/debug/evsieve"]

# Pass a delay to a input list of run_unittest to signify that it should wait a bit before
# sending the next events.
class Delay:
    period: float

    def __init__(self, period):
        self.period = period

def run_unittest(
    arguments: List[str],
    input: Dict[str, List[Union[Tuple[int, int, int], Delay]]],
    output: Dict[str, List[Tuple[int, int, int]]],
    auto_syn = True,
    expected_output = None,
):
    # Create virtual input devices.
    input_devices = dict()
    for path, events in input.items():
        non_delay_events = [
            event
            for event in events
            if not isinstance(event, Delay)
        ]
        type_capabilities = set([type for (type, _, _) in non_delay_events if type != e.EV_SYN])
        capabilities = {
            type: [
                code
                for (_type, code, _) in non_delay_events
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
        output_events = [list() for dev in output_devices]
        for device, events in input_devices.items():
            for event in events:
                if isinstance(event, Delay):
                    time.sleep(event.period)
                    # Read the output events at this point in time.
                    for ((output_device, _), events) in zip(output_devices, output_events):
                        try:
                            for event in output_device.read():
                                if event.type != e.EV_SYN:
                                    events.append(event)
                        except BlockingIOError:
                            pass
                    continue

                device.write(*event)
                if auto_syn:
                    device.syn()
        
        # Final read pass.
        time.sleep(0.01)
        for ((output_device, _), events) in zip(output_devices, output_events):
            try:
                for event in output_device.read():
                    if event.type != e.EV_SYN or not auto_syn:
                        events.append(event)
            except BlockingIOError:
                pass

        # Check whether the output devices have the expected events.
        success = True
        report_msg = ""
        column_format_str = " {:<30}| {}\n"

        for (events, (output_device, expected_events)) in zip(output_events, output_devices):
            report_msg += f"For the device {output_device.path}:\n"
            report_msg += column_format_str.format("Expected", "Received") + 60 * "-" + "\n"

            for event in events:
                event = (event.type, event.code, event.value)

                if expected_events:
                    expected_event = expected_events.pop(0)

                    report_msg += column_format_str.format(
                        format_event(expected_event), format_event(event)
                    )

                    if event != expected_event:
                        success = False
                else:
                    report_msg += column_format_str.format(
                        "(none)", format_event(event)
                    )
                    success = False

            if len(expected_events) > 0:
                success = False
                for missing_event in expected_events:
                    report_msg += column_format_str.format(format_event(missing_event), "(missing)")
        
        if not success:
            print(report_msg)
            raise Exception(f"Unit test failed.")

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


def format_event(event: Tuple[int, int, int]) -> str:
    "Formats a (type, code, value) event in a human-readable way."

    return f"({e.EV[event[0]]}, {e.bytype[event[0]][event[1]]}, {event[2]})"

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

def unittest_syn():
    run_unittest(
        ["--input", "/dev/input/by-id/unittest-syn-in", "grab=force",
        "--map", "key:x", "key:y", "key:z",
        "--output", "create-link=/dev/input/by-id/unittest-syn-out", "repeat=passive"],
        {
            "/dev/input/by-id/unittest-syn-in": [
                (e.EV_KEY, e.KEY_A, 1),
                (e.EV_SYN, 0, 0),
                (e.EV_KEY, e.KEY_A, 2),
                (e.EV_SYN, 0, 0),
                (e.EV_KEY, e.KEY_A, 0),
                (e.EV_SYN, 0, 0),
                (e.EV_KEY, e.KEY_B, 1),
                (e.EV_KEY, e.KEY_C, 1),
                (e.EV_SYN, 0, 0),
                (e.EV_KEY, e.KEY_B, 0),
                (e.EV_KEY, e.KEY_C, 0),
                (e.EV_SYN, 0, 0),

                (e.EV_KEY, e.KEY_D, 1),
                (e.EV_KEY, e.KEY_X, 1),
                (e.EV_SYN, 0, 0),
                (e.EV_KEY, e.KEY_X, 0),
                (e.EV_SYN, 0, 0),
                (e.EV_KEY, e.KEY_D, 0),
                (e.EV_SYN, 0, 0),
            ],
        },
        {
            "/dev/input/by-id/unittest-syn-out": [
                (e.EV_KEY, e.KEY_A, 1),
                (e.EV_SYN, 0, 0),
                (e.EV_KEY, e.KEY_A, 2),
                (e.EV_SYN, 0, 0),
                (e.EV_KEY, e.KEY_A, 0),
                (e.EV_SYN, 0, 0),
                (e.EV_KEY, e.KEY_B, 1),
                (e.EV_KEY, e.KEY_C, 1),
                (e.EV_SYN, 0, 0),
                (e.EV_KEY, e.KEY_B, 0),
                (e.EV_KEY, e.KEY_C, 0),
                (e.EV_SYN, 0, 0),

                (e.EV_KEY, e.KEY_D, 1),
                (e.EV_KEY, e.KEY_Y, 1),
                (e.EV_SYN, 0, 0),
                (e.EV_KEY, e.KEY_Z, 1),
                (e.EV_SYN, 0, 0),
                (e.EV_KEY, e.KEY_Y, 0),
                (e.EV_SYN, 0, 0),
                (e.EV_KEY, e.KEY_Z, 0),
                (e.EV_SYN, 0, 0),
                (e.EV_KEY, e.KEY_D, 0),
                (e.EV_SYN, 0, 0),
            ],
        },
        auto_syn=False,
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
                Delay(0.01),
                (e.EV_KEY, e.KEY_T, 1),
                (e.EV_KEY, e.KEY_T, 0),
                Delay(0.01),

                (e.EV_KEY, e.KEY_B, 1),
                (e.EV_KEY, e.KEY_B, 2),
                (e.EV_KEY, e.KEY_A, 1),
                (e.EV_KEY, e.KEY_A, 2),
                (e.EV_KEY, e.KEY_A, 0),
                (e.EV_KEY, e.KEY_B, 0),
                Delay(0.01),

                (e.EV_KEY, e.KEY_Q, 1),
                (e.EV_KEY, e.KEY_W, 1),
                (e.EV_KEY, e.KEY_Q, 2),
                (e.EV_KEY, e.KEY_W, 2),
                (e.EV_KEY, e.KEY_Q, 0),
                (e.EV_KEY, e.KEY_W, 0),
                (e.EV_KEY, e.KEY_Q, 1),
                (e.EV_KEY, e.KEY_Q, 0),
                Delay(0.01),

                (e.EV_KEY, e.KEY_X, 1),
                (e.EV_KEY, e.KEY_C, 1), 
                (e.EV_KEY, e.KEY_Z, 1),
                (e.EV_KEY, e.KEY_X, 0),
                (e.EV_KEY, e.KEY_C, 0),
                (e.EV_KEY, e.KEY_Z, 0),
                Delay(0.01),
                
                # Should not trigger: O is not pressed.
                (e.EV_KEY, e.KEY_O, 1),
                (e.EV_KEY, e.KEY_O, 0),
                (e.EV_KEY, e.KEY_I, 1),
                (e.EV_KEY, e.KEY_E, 1),
                (e.EV_KEY, e.KEY_E, 0),
                (e.EV_KEY, e.KEY_I, 0),
                Delay(0.01),

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
                Delay(0.01),
            ],
        },
        {},
        expected_output = "trigger1\ntrigger1\ntrigger2\ntrigger3\ntrigger4\ntrigger6\ntrigger6\n",
    )

def unittest_sequential_hook():
    run_unittest(
        ["--input", "/dev/input/by-id/unittest-sequential-hook-in", "grab=force",
        "--hook", "key:a", "sequential", "send-key=key:x",
        "--hook", "key:b", "key:c", "sequential", "send-key=key:y",
        "--hook", "key:d", "key:e", "key:f", "sequential", "send-key=key:z",
        "--hook", "key:d", "key:e", "key:f", "send-key=key:w",
        "--output", "create-link=/dev/input/by-id/unittest-sequential-hook-out", "repeat=passive"],
        {
            "/dev/input/by-id/unittest-sequential-hook-in": [
                (e.EV_KEY, e.KEY_A, 1),
                (e.EV_KEY, e.KEY_A, 0),

                # I don't know whether the next two groups describe the most sensible behaviour,
                # but they seem no less crazy than the alternative and they are how evsieve
                # behaves now, so that behaviour must be preserved.
                (e.EV_KEY, e.KEY_B, 1),
                (e.EV_KEY, e.KEY_C, 1),
                (e.EV_KEY, e.KEY_C, 0),
                (e.EV_KEY, e.KEY_C, 1),
                (e.EV_KEY, e.KEY_B, 0),
                (e.EV_KEY, e.KEY_C, 0),

                (e.EV_KEY, e.KEY_C, 1),
                (e.EV_KEY, e.KEY_B, 1),
                (e.EV_KEY, e.KEY_C, 0),
                (e.EV_KEY, e.KEY_C, 1),
                (e.EV_KEY, e.KEY_B, 0),
                (e.EV_KEY, e.KEY_C, 0),

                Delay(0.001),

                (e.EV_KEY, e.KEY_D, 1),
                (e.EV_KEY, e.KEY_F, 1),
                (e.EV_KEY, e.KEY_E, 1),
                (e.EV_KEY, e.KEY_F, 0),
                (e.EV_KEY, e.KEY_E, 0),
                (e.EV_KEY, e.KEY_D, 0),

                (e.EV_KEY, e.KEY_D, 1),
                (e.EV_KEY, e.KEY_E, 1),
                (e.EV_KEY, e.KEY_F, 1),
                (e.EV_KEY, e.KEY_E, 0),
                (e.EV_KEY, e.KEY_E, 1),
                (e.EV_KEY, e.KEY_E, 0),
                (e.EV_KEY, e.KEY_F, 0),
                (e.EV_KEY, e.KEY_D, 0),
            ]
        },
        {
            "/dev/input/by-id/unittest-sequential-hook-out": [
                (e.EV_KEY, e.KEY_A, 1),
                (e.EV_KEY, e.KEY_X, 1),
                (e.EV_KEY, e.KEY_X, 0),
                (e.EV_KEY, e.KEY_A, 0),

                (e.EV_KEY, e.KEY_B, 1),
                (e.EV_KEY, e.KEY_C, 1),
                (e.EV_KEY, e.KEY_Y, 1),
                (e.EV_KEY, e.KEY_Y, 0),
                (e.EV_KEY, e.KEY_C, 0),
                (e.EV_KEY, e.KEY_C, 1),
                (e.EV_KEY, e.KEY_Y, 1),
                (e.EV_KEY, e.KEY_Y, 0),
                (e.EV_KEY, e.KEY_B, 0),
                (e.EV_KEY, e.KEY_C, 0),

                (e.EV_KEY, e.KEY_C, 1),
                (e.EV_KEY, e.KEY_B, 1),
                (e.EV_KEY, e.KEY_C, 0),
                (e.EV_KEY, e.KEY_C, 1),
                (e.EV_KEY, e.KEY_Y, 1),
                (e.EV_KEY, e.KEY_Y, 0),
                (e.EV_KEY, e.KEY_B, 0),
                (e.EV_KEY, e.KEY_C, 0),

                (e.EV_KEY, e.KEY_D, 1),
                (e.EV_KEY, e.KEY_F, 1),
                (e.EV_KEY, e.KEY_E, 1),
                (e.EV_KEY, e.KEY_W, 1),
                (e.EV_KEY, e.KEY_W, 0),
                (e.EV_KEY, e.KEY_F, 0),
                (e.EV_KEY, e.KEY_E, 0),
                (e.EV_KEY, e.KEY_D, 0),

                (e.EV_KEY, e.KEY_D, 1),
                (e.EV_KEY, e.KEY_E, 1),
                (e.EV_KEY, e.KEY_F, 1),
                (e.EV_KEY, e.KEY_W, 1),
                (e.EV_KEY, e.KEY_Z, 1),
                (e.EV_KEY, e.KEY_Z, 0),
                (e.EV_KEY, e.KEY_W, 0),
                (e.EV_KEY, e.KEY_E, 0),
                (e.EV_KEY, e.KEY_E, 1),
                (e.EV_KEY, e.KEY_W, 1),
                (e.EV_KEY, e.KEY_W, 0),
                (e.EV_KEY, e.KEY_E, 0),
                (e.EV_KEY, e.KEY_F, 0),
                (e.EV_KEY, e.KEY_D, 0),
            ]
        },
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

def unittest_bynumber():
    run_unittest(
        ["--input", "/dev/input/by-id/unittest-bynumber-in", "grab=force",
        "--map", f"key:%{e.KEY_A}", f"key:%{e.KEY_B}",
        "--map", f"btn:%{e.BTN_LEFT}", f"key:%{e.KEY_C}",
        "--map", f"%{e.EV_KEY}:%{e.BTN_RIGHT}", f"abs:%{e.ABS_X}",
        "--output", f"%{e.EV_KEY}", "create-link=/dev/input/by-id/unittest-bynumber-out-1",
        "--output", f"%{e.EV_ABS}", "create-link=/dev/input/by-id/unittest-bynumber-out-2"],
        {
            "/dev/input/by-id/unittest-bynumber-in": [
                (e.EV_KEY, e.KEY_A, 1),
                (e.EV_KEY, e.KEY_A, 0),
                (e.EV_KEY, e.BTN_LEFT, 1),
                (e.EV_KEY, e.BTN_LEFT, 0),
                (e.EV_KEY, e.BTN_RIGHT, 1),
                (e.EV_KEY, e.BTN_RIGHT, 0),
            ],
        },
        {
            "/dev/input/by-id/unittest-bynumber-out-1": [
                (e.EV_KEY, e.KEY_B, 1),
                (e.EV_KEY, e.KEY_B, 0),
                (e.EV_KEY, e.KEY_C, 1),
                (e.EV_KEY, e.KEY_C, 0),
            ],
            "/dev/input/by-id/unittest-bynumber-out-2": [
                (e.EV_ABS, e.ABS_X, 1),
                (e.EV_ABS, e.ABS_X, 0),
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

def unittest_relative():
    run_unittest(
        ["--input", "/dev/input/by-id/unittest-relative-in", "grab=force",
        "--map", "abs:x", "rel:x:0.5d",
        "--map", "abs:y", "abs:y:-1.4x",
        "--map", "abs:rx", "abs:rx:1.5x+0.4d+1",
        "--output", "create-link=/dev/input/by-id/unittest-relative-out"],
        {
            "/dev/input/by-id/unittest-relative-in": [
                # Test evsieve's resistance to rounding errors: the first movement should be
                # rounded down, the second rounded up.
                (e.EV_ABS, e.ABS_X, 7),
                (e.EV_ABS, e.ABS_X, 10),
                (e.EV_ABS, e.ABS_X, 0),

                # Test absolute factors. Unlike delta-maps, these should always be rounded
                # by truncation.
                (e.EV_ABS, e.ABS_Y, 5),
                (e.EV_ABS, e.ABS_Y, 7),
                (e.EV_ABS, e.ABS_Y, 8),
                (e.EV_ABS, e.ABS_Y, -5),
                (e.EV_ABS, e.ABS_Y, 0),

                (e.EV_ABS, e.ABS_RX, 2),
                (e.EV_ABS, e.ABS_RX, 5),
                (e.EV_ABS, e.ABS_RX, 0),
                (e.EV_ABS, e.ABS_RX, 7),
            ],
        },
        {
            "/dev/input/by-id/unittest-relative-out": [
                (e.EV_REL, e.REL_X, 3),
                (e.EV_REL, e.REL_X, 2),
                (e.EV_REL, e.REL_X, -5),

                (e.EV_ABS, e.ABS_Y, -7),
                (e.EV_ABS, e.ABS_Y, -9),
                (e.EV_ABS, e.ABS_Y, -11),
                (e.EV_ABS, e.ABS_Y, 7),
                (e.EV_ABS, e.ABS_Y, 0),

                (e.EV_ABS, e.ABS_RX, 4),
                (e.EV_ABS, e.ABS_RX, 10),
                (e.EV_ABS, e.ABS_RX, -1),
                (e.EV_ABS, e.ABS_RX, 13),
            ],
        },
    )

def unittest_delay():
    run_unittest(
        ["--input", "/dev/input/by-id/unittest-delay-in-1", "grab=force",
        "--delay", "key:a", "key:b", "period=0.01",
        "--output", "create-link=/dev/input/by-id/unittest-delay-out-1"],
        {
            "/dev/input/by-id/unittest-delay-in-1": [
                (e.EV_KEY, e.KEY_A, 1),
                (e.EV_KEY, e.KEY_A, 2),
                (e.EV_KEY, e.KEY_A, 0),
                (e.EV_KEY, e.KEY_B, 1),
                (e.EV_KEY, e.KEY_B, 2),
                (e.EV_KEY, e.KEY_B, 0),
                Delay(0.005),
                (e.EV_KEY, e.KEY_C, 1),
                (e.EV_KEY, e.KEY_C, 0),
                Delay(0.01),

                (e.EV_KEY, e.KEY_A, 1),
                (e.EV_KEY, e.KEY_A, 2),
                (e.EV_KEY, e.KEY_A, 0),
                (e.EV_KEY, e.KEY_B, 1),
                (e.EV_KEY, e.KEY_B, 2),
                (e.EV_KEY, e.KEY_B, 0),
                Delay(0.015),
                (e.EV_KEY, e.KEY_C, 1),
                (e.EV_KEY, e.KEY_C, 0),
                Delay(0.005),
            ]
        },
        {
            "/dev/input/by-id/unittest-delay-out-1": [
                (e.EV_KEY, e.KEY_C, 1),
                (e.EV_KEY, e.KEY_C, 0),
                (e.EV_KEY, e.KEY_A, 1),
                (e.EV_KEY, e.KEY_A, 2),
                (e.EV_KEY, e.KEY_A, 0),
                (e.EV_KEY, e.KEY_B, 1),
                (e.EV_KEY, e.KEY_B, 2),
                (e.EV_KEY, e.KEY_B, 0),

                (e.EV_KEY, e.KEY_A, 1),
                (e.EV_KEY, e.KEY_A, 2),
                (e.EV_KEY, e.KEY_A, 0),
                (e.EV_KEY, e.KEY_B, 1),
                (e.EV_KEY, e.KEY_B, 2),
                (e.EV_KEY, e.KEY_B, 0),
                (e.EV_KEY, e.KEY_C, 1),
                (e.EV_KEY, e.KEY_C, 0),
            ],
        },
    )

def unittest_send_key():
    run_unittest(
        ["--input", "/dev/input/by-id/unittest-send-key-in", "domain=in", "grab=force",

        # We use --block to prevent capabilities from the input device from propagating to
        # the output device in order to test whether send-key generates the appropriate
        # capabilities.
        "--map", "key:a", "@out",
        "--map", "key:b", "@out",
        "--block", "@in",

        "--hook", "key:a", "key:b", "send-key=key:x", "send-key=key:y", "send-key=key:z",
        "--output", "@out", "create-link=/dev/input/by-id/unittest-send-key-out"],
        {
            "/dev/input/by-id/unittest-send-key-in": [
                (e.EV_KEY, e.KEY_A, 1),
                (e.EV_KEY, e.KEY_B, 1),
                (e.EV_KEY, e.KEY_A, 0),
                (e.EV_KEY, e.KEY_B, 0),
            ],
        },
        {
            "/dev/input/by-id/unittest-send-key-out": [
                (e.EV_KEY, e.KEY_A, 1),
                (e.EV_KEY, e.KEY_B, 1),
                (e.EV_KEY, e.KEY_X, 1),
                (e.EV_KEY, e.KEY_Y, 1),
                (e.EV_KEY, e.KEY_Z, 1),
                (e.EV_KEY, e.KEY_Z, 0),
                (e.EV_KEY, e.KEY_Y, 0),
                (e.EV_KEY, e.KEY_X, 0),
                (e.EV_KEY, e.KEY_A, 0),
                (e.EV_KEY, e.KEY_B, 0),
            ],
        },
    )

def unittest_withhold():
    run_unittest(
        ["--input", "/dev/input/by-id/unittest-withhold-in", "grab=force",
        "--hook", "key:a", "key:b", "send-key=key:x",
        "--withhold",
        "--hook", "key:c",
        "--map", "key:c", "key:c@bar",
        "--hook", "key:c@foo",
        "--hook", "key:d",
        "--hook", "key:e",
        "--withhold", "key:c", "key:d", "key:f",
        "--hook", "key:g", "key:h", "key:i",
        "--hook", "key:h", "key:j",
        "--withhold",
        "--map", "key:k", "key:k@foo",
        "--map", "key:l", "key:k@bar",
        "--hook", "key:k", "key:m",
        "--withhold",
        "--map", "key:k@bar", "key:l",
        "--output", "create-link=/dev/input/by-id/unittest-withhold-out"],
        {
            "/dev/input/by-id/unittest-withhold-in": [
                # Part 1
                (e.EV_KEY, e.KEY_A, 1),
                (e.EV_KEY, e.KEY_S, 1),
                (e.EV_KEY, e.KEY_S, 0),
                (e.EV_KEY, e.KEY_A, 0),

                (e.EV_KEY, e.KEY_B, 1),
                (e.EV_KEY, e.KEY_B, 0),

                (e.EV_KEY, e.KEY_T, 1),
                (e.EV_KEY, e.KEY_T, 0),

                # Part 2
                (e.EV_KEY, e.KEY_A, 1),
                (e.EV_KEY, e.KEY_S, 1),
                (e.EV_KEY, e.KEY_B, 1),
                (e.EV_KEY, e.KEY_S, 2),
                (e.EV_KEY, e.KEY_A, 0),
                (e.EV_KEY, e.KEY_S, 0),
                (e.EV_KEY, e.KEY_B, 0),

                # Part 3
                (e.EV_KEY, e.KEY_C, 1),
                (e.EV_KEY, e.KEY_C, 0),

                (e.EV_KEY, e.KEY_D, 1),
                (e.EV_KEY, e.KEY_D, 0),
                (e.EV_KEY, e.KEY_E, 1),
                (e.EV_KEY, e.KEY_E, 0),
                (e.EV_KEY, e.KEY_F, 1),
                (e.EV_KEY, e.KEY_F, 0),

                # Read events now so we don't overflow the buffer.
                Delay(0.01),

                # Part 4
                (e.EV_KEY, e.KEY_H, 1),
                (e.EV_KEY, e.KEY_H, 0),

                (e.EV_KEY, e.KEY_G, 1),
                (e.EV_KEY, e.KEY_H, 1),
                (e.EV_KEY, e.KEY_H, 0),
                (e.EV_KEY, e.KEY_I, 1),
                (e.EV_KEY, e.KEY_I, 0),
                (e.EV_KEY, e.KEY_G, 0),

                (e.EV_KEY, e.KEY_S, 1),
                (e.EV_KEY, e.KEY_S, 0),

                (e.EV_KEY, e.KEY_G, 1),
                (e.EV_KEY, e.KEY_H, 1),
                (e.EV_KEY, e.KEY_J, 1),
                (e.EV_KEY, e.KEY_G, 0),
                (e.EV_KEY, e.KEY_H, 0),
                (e.EV_KEY, e.KEY_J, 0),

                # Part 5
                (e.EV_KEY, e.KEY_K, 1),
                (e.EV_KEY, e.KEY_L, 1),
                (e.EV_KEY, e.KEY_K, 0),
                (e.EV_KEY, e.KEY_L, 0),

                (e.EV_KEY, e.KEY_S, 1),
                (e.EV_KEY, e.KEY_K, 1),
                (e.EV_KEY, e.KEY_L, 1),
                (e.EV_KEY, e.KEY_M, 1),
                (e.EV_KEY, e.KEY_K, 0),
                (e.EV_KEY, e.KEY_L, 0),
                (e.EV_KEY, e.KEY_M, 0),
                (e.EV_KEY, e.KEY_S, 0),
            ],
        },
        {
            "/dev/input/by-id/unittest-withhold-out": [
                # Part 1
                (e.EV_KEY, e.KEY_S, 1),
                (e.EV_KEY, e.KEY_S, 0),
                (e.EV_KEY, e.KEY_A, 1),
                (e.EV_KEY, e.KEY_A, 0),

                (e.EV_KEY, e.KEY_B, 1),
                (e.EV_KEY, e.KEY_B, 0),

                (e.EV_KEY, e.KEY_T, 1),
                (e.EV_KEY, e.KEY_T, 0),

                # Part 2
                (e.EV_KEY, e.KEY_S, 1),
                (e.EV_KEY, e.KEY_X, 1),
                (e.EV_KEY, e.KEY_S, 2),
                (e.EV_KEY, e.KEY_X, 0),
                (e.EV_KEY, e.KEY_S, 0),

                # Part 3
                (e.EV_KEY, e.KEY_C, 1),
                (e.EV_KEY, e.KEY_C, 0),

                (e.EV_KEY, e.KEY_E, 1),
                (e.EV_KEY, e.KEY_E, 0),
                (e.EV_KEY, e.KEY_F, 1),
                (e.EV_KEY, e.KEY_F, 0),

                # Part 4
                (e.EV_KEY, e.KEY_H, 1),
                (e.EV_KEY, e.KEY_H, 0),

                (e.EV_KEY, e.KEY_H, 1),
                (e.EV_KEY, e.KEY_H, 0),
                (e.EV_KEY, e.KEY_I, 1),
                (e.EV_KEY, e.KEY_I, 0),
                (e.EV_KEY, e.KEY_G, 1),
                (e.EV_KEY, e.KEY_G, 0),

                (e.EV_KEY, e.KEY_S, 1),
                (e.EV_KEY, e.KEY_S, 0),

                (e.EV_KEY, e.KEY_G, 1),
                (e.EV_KEY, e.KEY_G, 0),

                # Part 5
                (e.EV_KEY, e.KEY_K, 1),
                (e.EV_KEY, e.KEY_L, 1),
                (e.EV_KEY, e.KEY_K, 0),
                (e.EV_KEY, e.KEY_L, 0),

                (e.EV_KEY, e.KEY_S, 1),
                (e.EV_KEY, e.KEY_S, 0),
            ],
        },
    )


def unittest_withhold_2():
    # This unittest is currently disabled because the feature that it is meant to test
    # has been disabled. I'll leave it in the code for now because in the future it may
    # be desirable to re-enable that feature again.
    return

    run_unittest(
        ["--input", "/dev/input/by-id/unittest-withhold-2-in", "domain=foo", "grab=force",
        "--hook", "key:a", "key:b", "send-key=key:x",
        "--hook", "key:c", "send-key=key:b",
        "--withhold",
        "--hook", "key:f", "send-key=key:e",
        "--hook", "key:d", "key:e", "send-key=key:x",
        "--withhold",
        "--map", "key:z", "key:i@bar",
        "--hook", "key:g", "key:h", "send-key=key:v",
        "--hook", "key:i", "send-key=key:h",
        "--hook", "key:g", "key:h@foo", "send-key=key:y",
        "--withhold",
        "--output", "create-link=/dev/input/by-id/unittest-withhold-2-out"],
        {
            "/dev/input/by-id/unittest-withhold-2-in": [
                (e.EV_KEY, e.KEY_A, 1),
                (e.EV_KEY, e.KEY_B, 1),
                (e.EV_KEY, e.KEY_B, 0),
                (e.EV_KEY, e.KEY_A, 0),

                (e.EV_KEY, e.KEY_A, 1),
                (e.EV_KEY, e.KEY_C, 1),
                (e.EV_KEY, e.KEY_A, 0),
                (e.EV_KEY, e.KEY_C, 0),

                (e.EV_KEY, e.KEY_D, 1),
                (e.EV_KEY, e.KEY_F, 1),
                (e.EV_KEY, e.KEY_D, 0),
                (e.EV_KEY, e.KEY_F, 0),

                (e.EV_KEY, e.KEY_G, 1),
                (e.EV_KEY, e.KEY_H, 1),
                (e.EV_KEY, e.KEY_G, 0),
                (e.EV_KEY, e.KEY_H, 0),

                (e.EV_KEY, e.KEY_G, 1),
                (e.EV_KEY, e.KEY_I, 1),
                (e.EV_KEY, e.KEY_G, 0),
                (e.EV_KEY, e.KEY_I, 0),

                (e.EV_KEY, e.KEY_G, 1),
                (e.EV_KEY, e.KEY_Z, 1),
                (e.EV_KEY, e.KEY_G, 0),
                (e.EV_KEY, e.KEY_Z, 0),
            ],
        },
        {
            "/dev/input/by-id/unittest-withhold-2-out": [
                (e.EV_KEY, e.KEY_X, 1),
                (e.EV_KEY, e.KEY_X, 0),

                (e.EV_KEY, e.KEY_B, 1),
                (e.EV_KEY, e.KEY_A, 1),
                (e.EV_KEY, e.KEY_A, 0),
                (e.EV_KEY, e.KEY_B, 0),

                (e.EV_KEY, e.KEY_X, 1),
                (e.EV_KEY, e.KEY_X, 0),

                (e.EV_KEY, e.KEY_Y, 1),
                (e.EV_KEY, e.KEY_V, 1),
                (e.EV_KEY, e.KEY_V, 0),
                (e.EV_KEY, e.KEY_Y, 0),

                (e.EV_KEY, e.KEY_Y, 1),
                (e.EV_KEY, e.KEY_Y, 0),

                (e.EV_KEY, e.KEY_H, 1),
                (e.EV_KEY, e.KEY_G, 1),
                (e.EV_KEY, e.KEY_G, 0),
                (e.EV_KEY, e.KEY_H, 0),
            ],
        },
    )

def unittest_withhold_3():
    run_unittest(
        ["--input", "/dev/input/by-id/unittest-withhold-3-in", "domain=foo", "grab=force",
        "--hook", "key:a", "abs:x:1~5", "send-key=key:x",
        "--withhold", "key",
        "--output", "create-link=/dev/input/by-id/unittest-withhold-3-out"],
        {
            "/dev/input/by-id/unittest-withhold-3-in": [
                (e.EV_KEY, e.KEY_A, 1),
                (e.EV_KEY, e.KEY_B, 1),
                (e.EV_KEY, e.KEY_B, 0),
                (e.EV_KEY, e.KEY_A, 0),

                (e.EV_KEY, e.KEY_A, 1),
                (e.EV_ABS, e.ABS_X, 3),
                (e.EV_KEY, e.KEY_A, 0),

                (e.EV_KEY, e.KEY_A, 1),
                (e.EV_KEY, e.KEY_A, 0),
                (e.EV_ABS, e.ABS_X, 7),

                (e.EV_KEY, e.KEY_A, 1),
                (e.EV_KEY, e.KEY_A, 0),

            ],
        },
        {
            "/dev/input/by-id/unittest-withhold-3-out": [
                (e.EV_KEY, e.KEY_B, 1),
                (e.EV_KEY, e.KEY_B, 0),
                (e.EV_KEY, e.KEY_A, 1),
                (e.EV_KEY, e.KEY_A, 0),

                (e.EV_ABS, e.ABS_X, 3),
                (e.EV_KEY, e.KEY_X, 1),
                (e.EV_KEY, e.KEY_X, 0),

                (e.EV_KEY, e.KEY_X, 1),
                (e.EV_KEY, e.KEY_X, 0),
                (e.EV_ABS, e.ABS_X, 7),

                (e.EV_KEY, e.KEY_A, 1),
                (e.EV_KEY, e.KEY_A, 0),
            ],
        },
    )

def unittest_withhold_period():
    run_unittest(
        ["--input", "/dev/input/by-id/unittest-withhold-period-in", "grab=force",
        "--hook", "key:a", "key:b", "send-key=key:x", "period=0.04",
        "--hook", "key:a", "key:c", "key:d", "send-key=key:w", "period=0.05",
        "--withhold",
        "--output", "create-link=/dev/input/by-id/unittest-withhold-period-out"],
        {
            "/dev/input/by-id/unittest-withhold-period-in": [
                (e.EV_KEY, e.KEY_A, 1),
                Delay(0.02),
                (e.EV_KEY, e.KEY_Z, 1),
                (e.EV_KEY, e.KEY_Z, 0),
                (e.EV_KEY, e.KEY_A, 0),

                (e.EV_KEY, e.KEY_A, 1),
                Delay(0.05),
                (e.EV_KEY, e.KEY_Z, 1),
                (e.EV_KEY, e.KEY_Z, 0),
                (e.EV_KEY, e.KEY_A, 0),

                (e.EV_KEY, e.KEY_A, 1),
                Delay(0.02),
                (e.EV_KEY, e.KEY_B, 1),
                (e.EV_KEY, e.KEY_Y, 1),
                (e.EV_KEY, e.KEY_Y, 0),
                (e.EV_KEY, e.KEY_A, 0),
                (e.EV_KEY, e.KEY_B, 0),

                (e.EV_KEY, e.KEY_A, 1),
                Delay(0.05),
                (e.EV_KEY, e.KEY_B, 1),
                (e.EV_KEY, e.KEY_Z, 1),
                (e.EV_KEY, e.KEY_Z, 0),
                (e.EV_KEY, e.KEY_A, 0),
                (e.EV_KEY, e.KEY_B, 0),

                (e.EV_KEY, e.KEY_C, 1),
                (e.EV_KEY, e.KEY_D, 1),
                Delay(0.02),
                (e.EV_KEY, e.KEY_C, 0),
                (e.EV_KEY, e.KEY_Z, 1),
                (e.EV_KEY, e.KEY_A, 1),
                Delay(0.01),
                (e.EV_KEY, e.KEY_C, 1),
                Delay(0.03),
                (e.EV_KEY, e.KEY_Z, 0),
                (e.EV_KEY, e.KEY_C, 0),
                (e.EV_KEY, e.KEY_D, 0),
                (e.EV_KEY, e.KEY_A, 0),

                (e.EV_KEY, e.KEY_C, 1),
                Delay(0.03),
                (e.EV_KEY, e.KEY_D, 1),
                Delay(0.03),
                (e.EV_KEY, e.KEY_Y, 1),
                (e.EV_KEY, e.KEY_Y, 0),
                (e.EV_KEY, e.KEY_C, 0),
                (e.EV_KEY, e.KEY_A, 1),
                Delay(0.01),
                (e.EV_KEY, e.KEY_C, 1),
                (e.EV_KEY, e.KEY_C, 0),
                (e.EV_KEY, e.KEY_D, 0),
                (e.EV_KEY, e.KEY_A, 0),
            ],
        },
        {
            "/dev/input/by-id/unittest-withhold-period-out": [
                (e.EV_KEY, e.KEY_Z, 1),
                (e.EV_KEY, e.KEY_Z, 0),
                (e.EV_KEY, e.KEY_A, 1),
                (e.EV_KEY, e.KEY_A, 0),

                (e.EV_KEY, e.KEY_A, 1),
                (e.EV_KEY, e.KEY_Z, 1),
                (e.EV_KEY, e.KEY_Z, 0),
                (e.EV_KEY, e.KEY_A, 0),

                (e.EV_KEY, e.KEY_X, 1),
                (e.EV_KEY, e.KEY_Y, 1),
                (e.EV_KEY, e.KEY_Y, 0),
                (e.EV_KEY, e.KEY_X, 0),

                (e.EV_KEY, e.KEY_A, 1),
                (e.EV_KEY, e.KEY_Z, 1),
                (e.EV_KEY, e.KEY_Z, 0),
                (e.EV_KEY, e.KEY_A, 0),
                (e.EV_KEY, e.KEY_B, 1),
                (e.EV_KEY, e.KEY_B, 0),

                (e.EV_KEY, e.KEY_C, 1),
                (e.EV_KEY, e.KEY_C, 0),
                (e.EV_KEY, e.KEY_Z, 1),
                (e.EV_KEY, e.KEY_W, 1),
                (e.EV_KEY, e.KEY_Z, 0),
                (e.EV_KEY, e.KEY_W, 0),

                (e.EV_KEY, e.KEY_C, 1),
                (e.EV_KEY, e.KEY_Y, 1),
                (e.EV_KEY, e.KEY_Y, 0),
                (e.EV_KEY, e.KEY_C, 0),
                (e.EV_KEY, e.KEY_W, 1),
                (e.EV_KEY, e.KEY_W, 0),
            ],
        },
    )

def unittest_withhold_sequential():
    run_unittest(
        ["--input", "/dev/input/by-id/unittest-withhold-sequential-in", "grab=force",
        "--hook", "key:a", "key:b", "key:c", "send-key=key:d", "sequential",
        "--withhold",
        "--output", "create-link=/dev/input/by-id/unittest-withhold-sequential-out"],
        {
            "/dev/input/by-id/unittest-withhold-sequential-in": [
                (e.EV_KEY, e.KEY_A, 1),
                (e.EV_KEY, e.KEY_B, 1),
                (e.EV_KEY, e.KEY_F, 1),
                (e.EV_KEY, e.KEY_C, 1),

                (e.EV_KEY, e.KEY_F, 0),
                (e.EV_KEY, e.KEY_A, 0),
                (e.EV_KEY, e.KEY_B, 0),
                (e.EV_KEY, e.KEY_C, 0),

                (e.EV_KEY, e.KEY_A, 1),
                (e.EV_KEY, e.KEY_G, 1),
                (e.EV_KEY, e.KEY_A, 0),
                (e.EV_KEY, e.KEY_G, 0),

                (e.EV_KEY, e.KEY_B, 1),
                (e.EV_KEY, e.KEY_H, 1),
                (e.EV_KEY, e.KEY_B, 0),
                (e.EV_KEY, e.KEY_H, 0),

                (e.EV_KEY, e.KEY_A, 1),
                (e.EV_KEY, e.KEY_B, 1),
                (e.EV_KEY, e.KEY_E, 1),
                (e.EV_KEY, e.KEY_A, 0),
                (e.EV_KEY, e.KEY_E, 0),
                (e.EV_KEY, e.KEY_B, 0),
            ],
        },
        {
            "/dev/input/by-id/unittest-withhold-sequential-out": [
                (e.EV_KEY, e.KEY_F, 1),
                (e.EV_KEY, e.KEY_D, 1),
                (e.EV_KEY, e.KEY_F, 0),
                (e.EV_KEY, e.KEY_D, 0),

                (e.EV_KEY, e.KEY_G, 1),
                (e.EV_KEY, e.KEY_A, 1),
                (e.EV_KEY, e.KEY_A, 0),
                (e.EV_KEY, e.KEY_G, 0),

                (e.EV_KEY, e.KEY_B, 1),
                (e.EV_KEY, e.KEY_H, 1),
                (e.EV_KEY, e.KEY_B, 0),
                (e.EV_KEY, e.KEY_H, 0),

                (e.EV_KEY, e.KEY_E, 1),
                (e.EV_KEY, e.KEY_A, 1),
                (e.EV_KEY, e.KEY_B, 1),
                (e.EV_KEY, e.KEY_A, 0),
                (e.EV_KEY, e.KEY_E, 0),
                (e.EV_KEY, e.KEY_B, 0),
            ],
        },
    )

def unittest_withhold_sequential_2():
    run_unittest(
        ["--input", "/dev/input/by-id/unittest-withhold-sequential-2-in", "grab=force",
        "--hook", "key:a", "key:b", "key:c", "key:d", "key:e", "key:f", "send-key=key:x", "sequential",
        "--withhold",
        "--output", "create-link=/dev/input/by-id/unittest-withhold-sequential-2-out"],
        {
            "/dev/input/by-id/unittest-withhold-sequential-2-in": [
                (e.EV_KEY, e.KEY_A, 1),
                (e.EV_KEY, e.KEY_B, 1),
                (e.EV_KEY, e.KEY_C, 1),
                (e.EV_KEY, e.KEY_D, 1),
                (e.EV_KEY, e.KEY_E, 1),

                (e.EV_KEY, e.KEY_B, 0),
                
                (e.EV_KEY, e.KEY_A, 0),

                (e.EV_KEY, e.KEY_C, 0),
                (e.EV_KEY, e.KEY_D, 0),
                (e.EV_KEY, e.KEY_E, 0),
            ],
        },
        {
            "/dev/input/by-id/unittest-withhold-sequential-2-out": [
                (e.EV_KEY, e.KEY_B, 1),
                (e.EV_KEY, e.KEY_C, 1),
                (e.EV_KEY, e.KEY_D, 1),
                (e.EV_KEY, e.KEY_E, 1),

                (e.EV_KEY, e.KEY_B, 0),

                (e.EV_KEY, e.KEY_A, 1),
                (e.EV_KEY, e.KEY_A, 0),

                (e.EV_KEY, e.KEY_C, 0),
                (e.EV_KEY, e.KEY_D, 0),
                (e.EV_KEY, e.KEY_E, 0),
            ],
        },
    )


def unittest_rel_to_abs():
    run_unittest(
        ["--input", "/dev/input/by-id/unittest-rel-to-abs-in", "domain=foo", "grab=force",
        "--map", "key:r:1", "abs:x:100@foo",
        "--map", "key:s:1", "abs:x:200@bar",
        "--block", "key",
        "--rel-to-abs", "rel:x", "abs:x:0~1000",
        "--rel-to-abs", "rel:y", "abs:y:0~1000",
        "--output", "create-link=/dev/input/by-id/unittest-rel-to-abs-out"],
        {
            "/dev/input/by-id/unittest-rel-to-abs-in": [
                (e.EV_REL, e.REL_X, 10),
                (e.EV_REL, e.REL_X, 15),
                (e.EV_REL, e.REL_X, -5),

                (e.EV_REL, e.REL_Y, 10),
                (e.EV_REL, e.REL_Z, 10),
                
                (e.EV_KEY, e.KEY_R, 1),
                (e.EV_KEY, e.KEY_S, 1),
                (e.EV_REL, e.REL_X, 5),
                
                (e.EV_REL, e.REL_Y, 10),
                (e.EV_REL, e.REL_Z, 10),

                (e.EV_REL, e.REL_X, 2000),
                (e.EV_REL, e.REL_X, -200),
                (e.EV_REL, e.REL_X, -2000),
                (e.EV_REL, e.REL_X, 40),
            ],
        },
        {
            "/dev/input/by-id/unittest-rel-to-abs-out": [
                (e.EV_ABS, e.ABS_X, 10),
                (e.EV_ABS, e.ABS_X, 25),
                (e.EV_ABS, e.ABS_X, 20),
                
                (e.EV_ABS, e.ABS_Y, 10),
                (e.EV_REL, e.REL_Z, 10),

                (e.EV_ABS, e.ABS_X, 100),
                (e.EV_ABS, e.ABS_X, 200),
                (e.EV_ABS, e.ABS_X, 105),

                (e.EV_ABS, e.ABS_Y, 20),
                (e.EV_REL, e.REL_Z, 10),

                (e.EV_ABS, e.ABS_X, 1000),
                (e.EV_ABS, e.ABS_X, 800),
                (e.EV_ABS, e.ABS_X, 0),
                (e.EV_ABS, e.ABS_X, 40),
            ],
        },
    )

def unittest_scale():
    run_unittest(
        ["--input", "/dev/input/by-id/unittest-scale-in", "grab=force",
        "--scale", "rel:x", "rel:y", "factor=0.25",
        "--output", "create-link=/dev/input/by-id/unittest-scale-out"],
        {
            "/dev/input/by-id/unittest-scale-in": [
                (e.EV_REL, e.REL_Z, 1),
                (e.EV_REL, e.REL_X, 10), # Send 2, residual 0.5
                (e.EV_REL, e.REL_X, 10), # Send 3, residual 0

                (e.EV_REL, e.REL_Z, 2),
                (e.EV_REL, e.REL_Y, 1),  # Y residual 0.25
                (e.EV_REL, e.REL_X, 6),  # Send 1, residual 0.5
                (e.EV_REL, e.REL_Z, 3),
                (e.EV_REL, e.REL_Y, 1),  # Y residual 0.5
                (e.EV_REL, e.REL_X, -3), # Send -1, residual 0.75
                (e.EV_REL, e.REL_Z, 4),
                (e.EV_REL, e.REL_Y, 1),  # Y residual 0.75
                (e.EV_REL, e.REL_X, 1),  # Send 1, residual 0
                (e.EV_REL, e.REL_Z, 5),
                (e.EV_REL, e.REL_Y, 1),  # Send 1, Y residual 0
                (e.EV_REL, e.REL_X, 1),  # Send 0, residual 0.25 (dropped)
                (e.EV_REL, e.REL_Z, 6),
                (e.EV_REL, e.REL_X, -10),# Send -3, residual 0.75
                (e.EV_REL, e.REL_Z, 7),
                (e.EV_REL, e.REL_X, 1),  # Send 1, residual 0

                (e.EV_ABS, e.ABS_X, 4),
            ],
        },
        {
            "/dev/input/by-id/unittest-scale-out": [
                (e.EV_REL, e.REL_Z, 1),
                (e.EV_REL, e.REL_X, 2),
                (e.EV_REL, e.REL_X, 3),

                (e.EV_REL, e.REL_Z, 2),
                (e.EV_REL, e.REL_X, 1),  # Send 1, residual 0.5
                (e.EV_REL, e.REL_Z, 3),
                (e.EV_REL, e.REL_X, -1), # Send -1, residual 0.75
                (e.EV_REL, e.REL_Z, 4),
                (e.EV_REL, e.REL_X, 1),  # Send 1, residual 0
                (e.EV_REL, e.REL_Z, 5),
                (e.EV_REL, e.REL_Y, 1),  # Send 1, Y residual 0
                (e.EV_REL, e.REL_Z, 6),
                (e.EV_REL, e.REL_X, -3), # Send -2, residual 0.75
                (e.EV_REL, e.REL_Z, 7),
                (e.EV_REL, e.REL_X, 1),  # Send 1, residual 0

                (e.EV_ABS, e.ABS_X, 4),
            ],
        },
    )

def unittest_scale_2():
    run_unittest(
        ["--input", "/dev/input/by-id/unittest-scale-2-in", "grab=force",
        "--scale", "factor=2",
        "--output", "create-link=/dev/input/by-id/unittest-scale-2-out"],
        {
            "/dev/input/by-id/unittest-scale-2-in": [
                (e.EV_REL, e.REL_X, 1),
                (e.EV_REL, e.REL_Y, 3),
                (e.EV_KEY, e.KEY_A, 1),
                (e.EV_KEY, e.KEY_A, 2),
                (e.EV_KEY, e.KEY_A, 0),

                (e.EV_ABS, e.ABS_X, 4),
                (e.EV_ABS, e.ABS_X, -3),
            ],
        },
        {
            "/dev/input/by-id/unittest-scale-2-out": [
                (e.EV_REL, e.REL_X, 2),
                (e.EV_REL, e.REL_Y, 6),
                (e.EV_KEY, e.KEY_A, 1),
                (e.EV_KEY, e.KEY_A, 2),
                (e.EV_KEY, e.KEY_A, 0),

                (e.EV_ABS, e.ABS_X, 8),
                (e.EV_ABS, e.ABS_X, -6),
            ],
        },
    )

def unittest_withhold_repeat():
    # This unittest originates from issue #46 (https://github.com/KarsMulder/evsieve/issues/46)
    num_repeats = 3
    run_unittest(
        [
            "--input", "/dev/input/by-id/unittest-withhold-repeat-in", "grab", "domain=k1",
            "--hook", "key:leftmeta", "key:m", "sequential", "breaks-on=key::1", "send-key=key:leftctrl", "send-key=key:o", "exec-shell=echo something",
            "--withhold",
            "--output", "@k1", "create-link=/dev/input/by-id/unittest-withhold-repeat-out"],
        {
            "/dev/input/by-id/unittest-withhold-repeat-in": [
                (e.EV_KEY, e.KEY_LEFTMETA, 1),
            ] + num_repeats * [
                (e.EV_KEY, e.KEY_M, 1),
                (e.EV_KEY, e.KEY_M, 0),
            ] + [
                (e.EV_KEY, e.KEY_LEFTMETA, 0),
            ],
        },
        {
            "/dev/input/by-id/unittest-withhold-repeat-out": num_repeats * [
                (e.EV_KEY, e.KEY_LEFTCTRL, 1),
                (e.EV_KEY, e.KEY_O, 1),
                (e.EV_KEY, e.KEY_O, 0),
                (e.EV_KEY, e.KEY_LEFTCTRL, 0),
            ],
        },
        expected_output = num_repeats * "something\n"
    )

unittest_mirror()
unittest_syn()
unittest_capslock()
unittest_doublectrl()
unittest_filterbyoutput()
unittest_domain()
unittest_kbmousemap()
unittest_execshell()
unittest_sequential_hook()
unittest_toggle()
unittest_yield()
unittest_order()
unittest_namespace()
unittest_consistency()
unittest_type()
unittest_bynumber()
unittest_merge()
unittest_relative()
unittest_delay()
unittest_send_key()
unittest_withhold()
unittest_withhold_2()
unittest_withhold_3()
unittest_withhold_period()
unittest_withhold_sequential()
unittest_withhold_sequential_2()
unittest_rel_to_abs()
unittest_scale()
unittest_scale_2()
unittest_withhold_repeat()

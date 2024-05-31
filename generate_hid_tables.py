#!/usr/bin/env python3

import json
import os
import dataclasses
from typing import *

# Load the JSON file provided by the USB Implementers Forum to obtain the HID tables.
REPOSITORY_ROOT = os.path.dirname(os.path.abspath(__file__))
TABLES_JSON_PATH = os.path.join(REPOSITORY_ROOT, "data/HidUsageTables.json")
OUTPUT_CODE_PATH = os.path.join(REPOSITORY_ROOT, "src/data/hid_usage_tables.rs")

if not os.path.exists(TABLES_JSON_PATH):
    print("The file data/HidUsageTables.json was not found. Please obtain a copy of the file HidUsageTables.json according to the instructions in data/put_HidUsageTables_in_this_folder.txt.")
    exit(1)

with open(TABLES_JSON_PATH, "rt") as file:
    hid_data = json.load(file)

# Convert the tables to something that can be included in the Rust binary without requiring dynamic allocation.
@dataclasses.dataclass
class HidUsage:
    id: int
    name: str

@dataclasses.dataclass
class HidPage:
    id: int
    name: str
    usages: List[HidUsage]

pages: List[HidPage] = list()

for page in sorted(hid_data["UsagePages"], key=lambda page: page["Id"]):
    usages: List[HidUsage] = list()
    for usage in sorted(page["UsageIds"], key=lambda usage: usage["Id"]):
        usages.append(HidUsage(
            usage["Id"], usage["Name"]
        ))
    
    pages.append(HidPage(
        page["Id"], page["Name"], usages
    ))

# Serialize the above lists to Rust source code.
result = """// SPDX licence header intentionally missing.
//
// This file was automatically generated from the HID usage tables provided by the USB Implementor's Forum.
// I believe that the content of this file is reasonable necessary for the implementation of a feature in
// this product. These tables can be downloaded at https://usb.org/document-library/hid-usage-tables-15.
// The following license was present in the specification:
//
//     Copyright © 1996-2020, USB Implementers Forum
//     All rights reserved.
//
//     INTELLECTUAL PROPERTY DISCLAIMER
//
//     THIS SPECIFICATION IS PROVIDED “AS IS” WITH NO WARRANTIES WHATSOEVER INCLUDING ANY
//     WARRANTY OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE, OR ANY WARRANTY
//     OTHERWISE ARISING OUT OF ANY PROPOSAL, SPECIFICATION, OR SAMPLE.
//
//     TO THE MAXIMUM EXTENT OF USB IMPLEMENTERS FORUM’S RIGHTS, USB IMPLEMENTERS FORUM
//     HEREBY GRANTS A LICENSE UNDER COPYRIGHT TO REPRODUCE THIS SPECIFICATION FOR
//     INTERNAL USE ONLY (E.G., ONLY WITHIN THE COMPANY OR ORGANIZATION THAT PROPERLY
//     DOWNLOADED OR OTHERWISE OBTAINED THE SPECIFICATION FROM USB IMPLEMENTERS FORUM,
//     OR FOR AN INDIVIDUAL, ONLY FOR USE BY THAT INDIVIDUAL). THIS SPECIFICATION MAY NOT BE
//     REPUBLISHED EXTERNALLY OR OTHERWISE TO THE PUBLIC.
//
//     IT IS CONTEMPLATED THAT MANY IMPLEMENTATIONS OF THIS SPECIFICATION (E.G., IN A PRODUCT)
//     DO NOT REQUIRE A LICENSE TO USE THIS SPECIFICATION UNDER COPYRIGHT. FOR CLARITY,
//     HOWEVER, TO THE MAXIMUM EXTENT OF USB IMPLEMENTERS FORUM’S RIGHTS, USB
//     IMPLEMENTERS FORUM HEREBY GRANTS A LICENSE UNDER COPYRIGHT TO USE THIS SPECIFICATION
//     AS REASONABLY NECESSARY TO IMPLEMENT THIS SPECIFICATION (E.G., IN A PRODUCT).
//     NO OTHER LICENSE, EXPRESS OR IMPLIED, BY ESTOPPEL OR OTHERWISE, TO ANY PATENT OR
//     OTHER INTELLECTUAL PROPERTY RIGHTS IS GRANTED OR INTENDED HEREBY.
//
//     USB IMPLEMENTERS FORUM AND THE AUTHORS OF THIS SPECIFICATION DISCLAIM ALL LIABILITY,
//     INCLUDING LIABILITY FOR INFRINGEMENT OF PROPRIETARY RIGHTS, RELATING TO
//     IMPLEMENTATION OF INFORMATION IN THIS SPECIFICATION. AUTHORS OF THIS SPECIFICATION ALSO
//     DO NOT WARRANT OR REPRESENT THAT SUCH IMPLEMENTATION(S) WILL NOT INFRINGE SUCH
//     RIGHTS.
//
//     All product names are trademarks, registered trademarks, or service marks of their respective owners.
//     Please send comments via electronic mail to hidcomments‘at’usb.org, us the @ sign for ‘at’

use super::hid_usage::{HidPage, HidUsage};

"""

def render_str(value: str) -> str:
    return "\"" + value.replace("\\", "\\\\") + "\""

result += f"pub static HID_PAGES: &[HidPage] = &[\n"
for page in pages:
    result += f"""    HidPage {{
        id: {page.id},
        name: &{render_str(page.name)},
        usages: &["""
    for usage in page.usages:
        result += f"""
            HidUsage {{ id: {usage.id}, name: &{render_str(usage.name)} }},"""
    result += f"\n       ],\n    }},\n"


result += "];\n\n"

# Write the resulting code to a Rust file.
with open(OUTPUT_CODE_PATH, "wt") as file:
    file.write(result)

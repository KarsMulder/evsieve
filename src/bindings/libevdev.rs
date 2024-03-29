// SPDX-License-Identifier: MIT
// 
// Copyright © 2013 Red Hat, Inc.
// 
// Permission is hereby granted, free of charge, to any person obtaining a copy
// of this software and associated documentation files (the "Software"), to
// deal in the Software without restriction, including without limitation the
// rights to use, copy, modify, merge, publish, distribute, sublicense, and/or
// sell copies of the Software, and to permit persons to whom the Software is
// furnished to do so, subject to the following conditions:
// 
// The above copyright notice and this permission notice (including the next
// paragraph) shall be included in all copies or substantial portions of the
// Software.
// 
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
// IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING
// FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS
// IN THE SOFTWARE./* automatically generated by rust-bindgen 0.59.2 */

pub const EV_SYN: u32 = 0;
pub const EV_KEY: u32 = 1;
pub const EV_REL: u32 = 2;
pub const EV_ABS: u32 = 3;
pub const EV_MSC: u32 = 4;
pub const EV_SW: u32 = 5;
pub const EV_LED: u32 = 17;
pub const EV_SND: u32 = 18;
pub const EV_REP: u32 = 20;
pub const EV_FF: u32 = 21;
pub const EV_PWR: u32 = 22;
pub const EV_FF_STATUS: u32 = 23;
pub const EV_MAX: u32 = 31;
pub const EV_CNT: u32 = 32;
pub const MSC_SERIAL: u32 = 0;
pub const MSC_PULSELED: u32 = 1;
pub const MSC_GESTURE: u32 = 2;
pub const MSC_RAW: u32 = 3;
pub const MSC_SCAN: u32 = 4;
pub const MSC_TIMESTAMP: u32 = 5;
pub const MSC_MAX: u32 = 7;
pub const MSC_CNT: u32 = 8;
pub const REP_DELAY: u32 = 0;
pub const REP_PERIOD: u32 = 1;
pub const REP_MAX: u32 = 1;
pub const REP_CNT: u32 = 2;
pub const EV_VERSION: u32 = 65537;
pub type __time_t = ::std::os::raw::c_long;
pub type __suseconds_t = ::std::os::raw::c_long;
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct timeval {
    pub tv_sec: __time_t,
    pub tv_usec: __suseconds_t,
}
#[test]
fn bindgen_test_layout_timeval() {
    assert_eq!(
        ::std::mem::size_of::<timeval>(),
        16usize,
        concat!("Size of: ", stringify!(timeval))
    );
    assert_eq!(
        ::std::mem::align_of::<timeval>(),
        8usize,
        concat!("Alignment of ", stringify!(timeval))
    );
    assert_eq!(
        unsafe { &(*(::std::ptr::null::<timeval>())).tv_sec as *const _ as usize },
        0usize,
        concat!(
            "Offset of field: ",
            stringify!(timeval),
            "::",
            stringify!(tv_sec)
        )
    );
    assert_eq!(
        unsafe { &(*(::std::ptr::null::<timeval>())).tv_usec as *const _ as usize },
        8usize,
        concat!(
            "Offset of field: ",
            stringify!(timeval),
            "::",
            stringify!(tv_usec)
        )
    );
}
pub type size_t = ::std::os::raw::c_ulong;
pub type __u16 = ::std::os::raw::c_ushort;
pub type __s32 = ::std::os::raw::c_int;
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct input_event {
    pub time: timeval,
    pub type_: __u16,
    pub code: __u16,
    pub value: __s32,
}
#[test]
fn bindgen_test_layout_input_event() {
    assert_eq!(
        ::std::mem::size_of::<input_event>(),
        24usize,
        concat!("Size of: ", stringify!(input_event))
    );
    assert_eq!(
        ::std::mem::align_of::<input_event>(),
        8usize,
        concat!("Alignment of ", stringify!(input_event))
    );
    assert_eq!(
        unsafe { &(*(::std::ptr::null::<input_event>())).time as *const _ as usize },
        0usize,
        concat!(
            "Offset of field: ",
            stringify!(input_event),
            "::",
            stringify!(time)
        )
    );
    assert_eq!(
        unsafe { &(*(::std::ptr::null::<input_event>())).type_ as *const _ as usize },
        16usize,
        concat!(
            "Offset of field: ",
            stringify!(input_event),
            "::",
            stringify!(type_)
        )
    );
    assert_eq!(
        unsafe { &(*(::std::ptr::null::<input_event>())).code as *const _ as usize },
        18usize,
        concat!(
            "Offset of field: ",
            stringify!(input_event),
            "::",
            stringify!(code)
        )
    );
    assert_eq!(
        unsafe { &(*(::std::ptr::null::<input_event>())).value as *const _ as usize },
        20usize,
        concat!(
            "Offset of field: ",
            stringify!(input_event),
            "::",
            stringify!(value)
        )
    );
}
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct input_absinfo {
    pub value: __s32,
    pub minimum: __s32,
    pub maximum: __s32,
    pub fuzz: __s32,
    pub flat: __s32,
    pub resolution: __s32,
}
#[test]
fn bindgen_test_layout_input_absinfo() {
    assert_eq!(
        ::std::mem::size_of::<input_absinfo>(),
        24usize,
        concat!("Size of: ", stringify!(input_absinfo))
    );
    assert_eq!(
        ::std::mem::align_of::<input_absinfo>(),
        4usize,
        concat!("Alignment of ", stringify!(input_absinfo))
    );
    assert_eq!(
        unsafe { &(*(::std::ptr::null::<input_absinfo>())).value as *const _ as usize },
        0usize,
        concat!(
            "Offset of field: ",
            stringify!(input_absinfo),
            "::",
            stringify!(value)
        )
    );
    assert_eq!(
        unsafe { &(*(::std::ptr::null::<input_absinfo>())).minimum as *const _ as usize },
        4usize,
        concat!(
            "Offset of field: ",
            stringify!(input_absinfo),
            "::",
            stringify!(minimum)
        )
    );
    assert_eq!(
        unsafe { &(*(::std::ptr::null::<input_absinfo>())).maximum as *const _ as usize },
        8usize,
        concat!(
            "Offset of field: ",
            stringify!(input_absinfo),
            "::",
            stringify!(maximum)
        )
    );
    assert_eq!(
        unsafe { &(*(::std::ptr::null::<input_absinfo>())).fuzz as *const _ as usize },
        12usize,
        concat!(
            "Offset of field: ",
            stringify!(input_absinfo),
            "::",
            stringify!(fuzz)
        )
    );
    assert_eq!(
        unsafe { &(*(::std::ptr::null::<input_absinfo>())).flat as *const _ as usize },
        16usize,
        concat!(
            "Offset of field: ",
            stringify!(input_absinfo),
            "::",
            stringify!(flat)
        )
    );
    assert_eq!(
        unsafe { &(*(::std::ptr::null::<input_absinfo>())).resolution as *const _ as usize },
        20usize,
        concat!(
            "Offset of field: ",
            stringify!(input_absinfo),
            "::",
            stringify!(resolution)
        )
    );
}
pub type va_list = __builtin_va_list;
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct libevdev {
    _unused: [u8; 0],
}
pub const libevdev_read_flag_LIBEVDEV_READ_FLAG_SYNC: libevdev_read_flag = 1;
pub const libevdev_read_flag_LIBEVDEV_READ_FLAG_NORMAL: libevdev_read_flag = 2;
pub const libevdev_read_flag_LIBEVDEV_READ_FLAG_FORCE_SYNC: libevdev_read_flag = 4;
pub const libevdev_read_flag_LIBEVDEV_READ_FLAG_BLOCKING: libevdev_read_flag = 8;
pub type libevdev_read_flag = ::std::os::raw::c_uint;
extern "C" {
    pub fn libevdev_new() -> *mut libevdev;
}
extern "C" {
    pub fn libevdev_new_from_fd(
        fd: ::std::os::raw::c_int,
        dev: *mut *mut libevdev,
    ) -> ::std::os::raw::c_int;
}
extern "C" {
    pub fn libevdev_free(dev: *mut libevdev);
}
pub const libevdev_log_priority_LIBEVDEV_LOG_ERROR: libevdev_log_priority = 10;
pub const libevdev_log_priority_LIBEVDEV_LOG_INFO: libevdev_log_priority = 20;
pub const libevdev_log_priority_LIBEVDEV_LOG_DEBUG: libevdev_log_priority = 30;
pub type libevdev_log_priority = ::std::os::raw::c_uint;
pub type libevdev_log_func_t = ::std::option::Option<
    unsafe extern "C" fn(
        priority: libevdev_log_priority,
        data: *mut ::std::os::raw::c_void,
        file: *const ::std::os::raw::c_char,
        line: ::std::os::raw::c_int,
        func: *const ::std::os::raw::c_char,
        format: *const ::std::os::raw::c_char,
        args: *mut __va_list_tag,
    ),
>;
extern "C" {
    pub fn libevdev_set_log_function(
        logfunc: libevdev_log_func_t,
        data: *mut ::std::os::raw::c_void,
    );
}
extern "C" {
    pub fn libevdev_set_log_priority(priority: libevdev_log_priority);
}
extern "C" {
    pub fn libevdev_get_log_priority() -> libevdev_log_priority;
}
pub type libevdev_device_log_func_t = ::std::option::Option<
    unsafe extern "C" fn(
        dev: *const libevdev,
        priority: libevdev_log_priority,
        data: *mut ::std::os::raw::c_void,
        file: *const ::std::os::raw::c_char,
        line: ::std::os::raw::c_int,
        func: *const ::std::os::raw::c_char,
        format: *const ::std::os::raw::c_char,
        args: *mut __va_list_tag,
    ),
>;
extern "C" {
    pub fn libevdev_set_device_log_function(
        dev: *mut libevdev,
        logfunc: libevdev_device_log_func_t,
        priority: libevdev_log_priority,
        data: *mut ::std::os::raw::c_void,
    );
}
pub const libevdev_grab_mode_LIBEVDEV_GRAB: libevdev_grab_mode = 3;
pub const libevdev_grab_mode_LIBEVDEV_UNGRAB: libevdev_grab_mode = 4;
pub type libevdev_grab_mode = ::std::os::raw::c_uint;
extern "C" {
    pub fn libevdev_grab(dev: *mut libevdev, grab: libevdev_grab_mode) -> ::std::os::raw::c_int;
}
extern "C" {
    pub fn libevdev_set_fd(dev: *mut libevdev, fd: ::std::os::raw::c_int) -> ::std::os::raw::c_int;
}
extern "C" {
    pub fn libevdev_change_fd(
        dev: *mut libevdev,
        fd: ::std::os::raw::c_int,
    ) -> ::std::os::raw::c_int;
}
extern "C" {
    pub fn libevdev_get_fd(dev: *const libevdev) -> ::std::os::raw::c_int;
}
pub const libevdev_read_status_LIBEVDEV_READ_STATUS_SUCCESS: libevdev_read_status = 0;
pub const libevdev_read_status_LIBEVDEV_READ_STATUS_SYNC: libevdev_read_status = 1;
pub type libevdev_read_status = ::std::os::raw::c_uint;
extern "C" {
    pub fn libevdev_next_event(
        dev: *mut libevdev,
        flags: ::std::os::raw::c_uint,
        ev: *mut input_event,
    ) -> ::std::os::raw::c_int;
}
extern "C" {
    pub fn libevdev_has_event_pending(dev: *mut libevdev) -> ::std::os::raw::c_int;
}
extern "C" {
    pub fn libevdev_get_name(dev: *const libevdev) -> *const ::std::os::raw::c_char;
}
extern "C" {
    pub fn libevdev_set_name(dev: *mut libevdev, name: *const ::std::os::raw::c_char);
}
extern "C" {
    pub fn libevdev_get_phys(dev: *const libevdev) -> *const ::std::os::raw::c_char;
}
extern "C" {
    pub fn libevdev_set_phys(dev: *mut libevdev, phys: *const ::std::os::raw::c_char);
}
extern "C" {
    pub fn libevdev_get_uniq(dev: *const libevdev) -> *const ::std::os::raw::c_char;
}
extern "C" {
    pub fn libevdev_set_uniq(dev: *mut libevdev, uniq: *const ::std::os::raw::c_char);
}
extern "C" {
    pub fn libevdev_get_id_product(dev: *const libevdev) -> ::std::os::raw::c_int;
}
extern "C" {
    pub fn libevdev_set_id_product(dev: *mut libevdev, product_id: ::std::os::raw::c_int);
}
extern "C" {
    pub fn libevdev_get_id_vendor(dev: *const libevdev) -> ::std::os::raw::c_int;
}
extern "C" {
    pub fn libevdev_set_id_vendor(dev: *mut libevdev, vendor_id: ::std::os::raw::c_int);
}
extern "C" {
    pub fn libevdev_get_id_bustype(dev: *const libevdev) -> ::std::os::raw::c_int;
}
extern "C" {
    pub fn libevdev_set_id_bustype(dev: *mut libevdev, bustype: ::std::os::raw::c_int);
}
extern "C" {
    pub fn libevdev_get_id_version(dev: *const libevdev) -> ::std::os::raw::c_int;
}
extern "C" {
    pub fn libevdev_set_id_version(dev: *mut libevdev, version: ::std::os::raw::c_int);
}
extern "C" {
    pub fn libevdev_get_driver_version(dev: *const libevdev) -> ::std::os::raw::c_int;
}
extern "C" {
    pub fn libevdev_has_property(
        dev: *const libevdev,
        prop: ::std::os::raw::c_uint,
    ) -> ::std::os::raw::c_int;
}
extern "C" {
    pub fn libevdev_enable_property(
        dev: *mut libevdev,
        prop: ::std::os::raw::c_uint,
    ) -> ::std::os::raw::c_int;
}
extern "C" {
    pub fn libevdev_disable_property(
        dev: *mut libevdev,
        prop: ::std::os::raw::c_uint,
    ) -> ::std::os::raw::c_int;
}
extern "C" {
    pub fn libevdev_has_event_type(
        dev: *const libevdev,
        type_: ::std::os::raw::c_uint,
    ) -> ::std::os::raw::c_int;
}
extern "C" {
    pub fn libevdev_has_event_code(
        dev: *const libevdev,
        type_: ::std::os::raw::c_uint,
        code: ::std::os::raw::c_uint,
    ) -> ::std::os::raw::c_int;
}
extern "C" {
    pub fn libevdev_get_abs_minimum(
        dev: *const libevdev,
        code: ::std::os::raw::c_uint,
    ) -> ::std::os::raw::c_int;
}
extern "C" {
    pub fn libevdev_get_abs_maximum(
        dev: *const libevdev,
        code: ::std::os::raw::c_uint,
    ) -> ::std::os::raw::c_int;
}
extern "C" {
    pub fn libevdev_get_abs_fuzz(
        dev: *const libevdev,
        code: ::std::os::raw::c_uint,
    ) -> ::std::os::raw::c_int;
}
extern "C" {
    pub fn libevdev_get_abs_flat(
        dev: *const libevdev,
        code: ::std::os::raw::c_uint,
    ) -> ::std::os::raw::c_int;
}
extern "C" {
    pub fn libevdev_get_abs_resolution(
        dev: *const libevdev,
        code: ::std::os::raw::c_uint,
    ) -> ::std::os::raw::c_int;
}
extern "C" {
    pub fn libevdev_get_abs_info(
        dev: *const libevdev,
        code: ::std::os::raw::c_uint,
    ) -> *const input_absinfo;
}
extern "C" {
    pub fn libevdev_get_event_value(
        dev: *const libevdev,
        type_: ::std::os::raw::c_uint,
        code: ::std::os::raw::c_uint,
    ) -> ::std::os::raw::c_int;
}
extern "C" {
    pub fn libevdev_set_event_value(
        dev: *mut libevdev,
        type_: ::std::os::raw::c_uint,
        code: ::std::os::raw::c_uint,
        value: ::std::os::raw::c_int,
    ) -> ::std::os::raw::c_int;
}
extern "C" {
    pub fn libevdev_fetch_event_value(
        dev: *const libevdev,
        type_: ::std::os::raw::c_uint,
        code: ::std::os::raw::c_uint,
        value: *mut ::std::os::raw::c_int,
    ) -> ::std::os::raw::c_int;
}
extern "C" {
    pub fn libevdev_get_slot_value(
        dev: *const libevdev,
        slot: ::std::os::raw::c_uint,
        code: ::std::os::raw::c_uint,
    ) -> ::std::os::raw::c_int;
}
extern "C" {
    pub fn libevdev_set_slot_value(
        dev: *mut libevdev,
        slot: ::std::os::raw::c_uint,
        code: ::std::os::raw::c_uint,
        value: ::std::os::raw::c_int,
    ) -> ::std::os::raw::c_int;
}
extern "C" {
    pub fn libevdev_fetch_slot_value(
        dev: *const libevdev,
        slot: ::std::os::raw::c_uint,
        code: ::std::os::raw::c_uint,
        value: *mut ::std::os::raw::c_int,
    ) -> ::std::os::raw::c_int;
}
extern "C" {
    pub fn libevdev_get_num_slots(dev: *const libevdev) -> ::std::os::raw::c_int;
}
extern "C" {
    pub fn libevdev_get_current_slot(dev: *const libevdev) -> ::std::os::raw::c_int;
}
extern "C" {
    pub fn libevdev_set_abs_minimum(
        dev: *mut libevdev,
        code: ::std::os::raw::c_uint,
        val: ::std::os::raw::c_int,
    );
}
extern "C" {
    pub fn libevdev_set_abs_maximum(
        dev: *mut libevdev,
        code: ::std::os::raw::c_uint,
        val: ::std::os::raw::c_int,
    );
}
extern "C" {
    pub fn libevdev_set_abs_fuzz(
        dev: *mut libevdev,
        code: ::std::os::raw::c_uint,
        val: ::std::os::raw::c_int,
    );
}
extern "C" {
    pub fn libevdev_set_abs_flat(
        dev: *mut libevdev,
        code: ::std::os::raw::c_uint,
        val: ::std::os::raw::c_int,
    );
}
extern "C" {
    pub fn libevdev_set_abs_resolution(
        dev: *mut libevdev,
        code: ::std::os::raw::c_uint,
        val: ::std::os::raw::c_int,
    );
}
extern "C" {
    pub fn libevdev_set_abs_info(
        dev: *mut libevdev,
        code: ::std::os::raw::c_uint,
        abs: *const input_absinfo,
    );
}
extern "C" {
    pub fn libevdev_enable_event_type(
        dev: *mut libevdev,
        type_: ::std::os::raw::c_uint,
    ) -> ::std::os::raw::c_int;
}
extern "C" {
    pub fn libevdev_disable_event_type(
        dev: *mut libevdev,
        type_: ::std::os::raw::c_uint,
    ) -> ::std::os::raw::c_int;
}
extern "C" {
    pub fn libevdev_enable_event_code(
        dev: *mut libevdev,
        type_: ::std::os::raw::c_uint,
        code: ::std::os::raw::c_uint,
        data: *const ::std::os::raw::c_void,
    ) -> ::std::os::raw::c_int;
}
extern "C" {
    pub fn libevdev_disable_event_code(
        dev: *mut libevdev,
        type_: ::std::os::raw::c_uint,
        code: ::std::os::raw::c_uint,
    ) -> ::std::os::raw::c_int;
}
extern "C" {
    pub fn libevdev_kernel_set_abs_info(
        dev: *mut libevdev,
        code: ::std::os::raw::c_uint,
        abs: *const input_absinfo,
    ) -> ::std::os::raw::c_int;
}
pub const libevdev_led_value_LIBEVDEV_LED_ON: libevdev_led_value = 3;
pub const libevdev_led_value_LIBEVDEV_LED_OFF: libevdev_led_value = 4;
pub type libevdev_led_value = ::std::os::raw::c_uint;
extern "C" {
    pub fn libevdev_kernel_set_led_value(
        dev: *mut libevdev,
        code: ::std::os::raw::c_uint,
        value: libevdev_led_value,
    ) -> ::std::os::raw::c_int;
}
extern "C" {
    pub fn libevdev_kernel_set_led_values(dev: *mut libevdev, ...) -> ::std::os::raw::c_int;
}
extern "C" {
    pub fn libevdev_set_clock_id(
        dev: *mut libevdev,
        clockid: ::std::os::raw::c_int,
    ) -> ::std::os::raw::c_int;
}
extern "C" {
    pub fn libevdev_event_is_type(
        ev: *const input_event,
        type_: ::std::os::raw::c_uint,
    ) -> ::std::os::raw::c_int;
}
extern "C" {
    pub fn libevdev_event_is_code(
        ev: *const input_event,
        type_: ::std::os::raw::c_uint,
        code: ::std::os::raw::c_uint,
    ) -> ::std::os::raw::c_int;
}
extern "C" {
    pub fn libevdev_event_type_get_name(
        type_: ::std::os::raw::c_uint,
    ) -> *const ::std::os::raw::c_char;
}
extern "C" {
    pub fn libevdev_event_code_get_name(
        type_: ::std::os::raw::c_uint,
        code: ::std::os::raw::c_uint,
    ) -> *const ::std::os::raw::c_char;
}
extern "C" {
    pub fn libevdev_event_value_get_name(
        type_: ::std::os::raw::c_uint,
        code: ::std::os::raw::c_uint,
        value: ::std::os::raw::c_int,
    ) -> *const ::std::os::raw::c_char;
}
extern "C" {
    pub fn libevdev_property_get_name(
        prop: ::std::os::raw::c_uint,
    ) -> *const ::std::os::raw::c_char;
}
extern "C" {
    pub fn libevdev_event_type_get_max(type_: ::std::os::raw::c_uint) -> ::std::os::raw::c_int;
}
extern "C" {
    pub fn libevdev_event_type_from_name(
        name: *const ::std::os::raw::c_char,
    ) -> ::std::os::raw::c_int;
}
extern "C" {
    pub fn libevdev_event_type_from_name_n(
        name: *const ::std::os::raw::c_char,
        len: size_t,
    ) -> ::std::os::raw::c_int;
}
extern "C" {
    pub fn libevdev_event_code_from_name(
        type_: ::std::os::raw::c_uint,
        name: *const ::std::os::raw::c_char,
    ) -> ::std::os::raw::c_int;
}
extern "C" {
    pub fn libevdev_event_code_from_name_n(
        type_: ::std::os::raw::c_uint,
        name: *const ::std::os::raw::c_char,
        len: size_t,
    ) -> ::std::os::raw::c_int;
}
extern "C" {
    pub fn libevdev_event_value_from_name(
        type_: ::std::os::raw::c_uint,
        code: ::std::os::raw::c_uint,
        name: *const ::std::os::raw::c_char,
    ) -> ::std::os::raw::c_int;
}
extern "C" {
    pub fn libevdev_event_type_from_code_name(
        name: *const ::std::os::raw::c_char,
    ) -> ::std::os::raw::c_int;
}
extern "C" {
    pub fn libevdev_event_type_from_code_name_n(
        name: *const ::std::os::raw::c_char,
        len: size_t,
    ) -> ::std::os::raw::c_int;
}
extern "C" {
    pub fn libevdev_event_code_from_code_name(
        name: *const ::std::os::raw::c_char,
    ) -> ::std::os::raw::c_int;
}
extern "C" {
    pub fn libevdev_event_code_from_code_name_n(
        name: *const ::std::os::raw::c_char,
        len: size_t,
    ) -> ::std::os::raw::c_int;
}
extern "C" {
    pub fn libevdev_event_value_from_name_n(
        type_: ::std::os::raw::c_uint,
        code: ::std::os::raw::c_uint,
        name: *const ::std::os::raw::c_char,
        len: size_t,
    ) -> ::std::os::raw::c_int;
}
extern "C" {
    pub fn libevdev_property_from_name(
        name: *const ::std::os::raw::c_char,
    ) -> ::std::os::raw::c_int;
}
extern "C" {
    pub fn libevdev_property_from_name_n(
        name: *const ::std::os::raw::c_char,
        len: size_t,
    ) -> ::std::os::raw::c_int;
}
extern "C" {
    pub fn libevdev_get_repeat(
        dev: *const libevdev,
        delay: *mut ::std::os::raw::c_int,
        period: *mut ::std::os::raw::c_int,
    ) -> ::std::os::raw::c_int;
}
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct libevdev_uinput {
    _unused: [u8; 0],
}
pub const libevdev_uinput_open_mode_LIBEVDEV_UINPUT_OPEN_MANAGED: libevdev_uinput_open_mode = -2;
pub type libevdev_uinput_open_mode = ::std::os::raw::c_int;
extern "C" {
    pub fn libevdev_uinput_create_from_device(
        dev: *const libevdev,
        uinput_fd: ::std::os::raw::c_int,
        uinput_dev: *mut *mut libevdev_uinput,
    ) -> ::std::os::raw::c_int;
}
extern "C" {
    pub fn libevdev_uinput_destroy(uinput_dev: *mut libevdev_uinput);
}
extern "C" {
    pub fn libevdev_uinput_get_fd(uinput_dev: *const libevdev_uinput) -> ::std::os::raw::c_int;
}
extern "C" {
    pub fn libevdev_uinput_get_syspath(
        uinput_dev: *mut libevdev_uinput,
    ) -> *const ::std::os::raw::c_char;
}
extern "C" {
    pub fn libevdev_uinput_get_devnode(
        uinput_dev: *mut libevdev_uinput,
    ) -> *const ::std::os::raw::c_char;
}
extern "C" {
    pub fn libevdev_uinput_write_event(
        uinput_dev: *const libevdev_uinput,
        type_: ::std::os::raw::c_uint,
        code: ::std::os::raw::c_uint,
        value: ::std::os::raw::c_int,
    ) -> ::std::os::raw::c_int;
}
pub type __builtin_va_list = [__va_list_tag; 1usize];
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct __va_list_tag {
    pub gp_offset: ::std::os::raw::c_uint,
    pub fp_offset: ::std::os::raw::c_uint,
    pub overflow_arg_area: *mut ::std::os::raw::c_void,
    pub reg_save_area: *mut ::std::os::raw::c_void,
}
#[test]
fn bindgen_test_layout___va_list_tag() {
    assert_eq!(
        ::std::mem::size_of::<__va_list_tag>(),
        24usize,
        concat!("Size of: ", stringify!(__va_list_tag))
    );
    assert_eq!(
        ::std::mem::align_of::<__va_list_tag>(),
        8usize,
        concat!("Alignment of ", stringify!(__va_list_tag))
    );
    assert_eq!(
        unsafe { &(*(::std::ptr::null::<__va_list_tag>())).gp_offset as *const _ as usize },
        0usize,
        concat!(
            "Offset of field: ",
            stringify!(__va_list_tag),
            "::",
            stringify!(gp_offset)
        )
    );
    assert_eq!(
        unsafe { &(*(::std::ptr::null::<__va_list_tag>())).fp_offset as *const _ as usize },
        4usize,
        concat!(
            "Offset of field: ",
            stringify!(__va_list_tag),
            "::",
            stringify!(fp_offset)
        )
    );
    assert_eq!(
        unsafe { &(*(::std::ptr::null::<__va_list_tag>())).overflow_arg_area as *const _ as usize },
        8usize,
        concat!(
            "Offset of field: ",
            stringify!(__va_list_tag),
            "::",
            stringify!(overflow_arg_area)
        )
    );
    assert_eq!(
        unsafe { &(*(::std::ptr::null::<__va_list_tag>())).reg_save_area as *const _ as usize },
        16usize,
        concat!(
            "Offset of field: ",
            stringify!(__va_list_tag),
            "::",
            stringify!(reg_save_area)
        )
    );
}

// SPDX-License-Identifier: GPL-2.0-or-later

//! A reimplementation of the standard library's time module that is guaranteed to correspond
//! to clock_gettime (Monotonic Clock). Although the standard library corresponds to that
//! as well, the documentation says that it may change over time, therefore we need our own
//! time module.

use std::mem::MaybeUninit;
use std::convert::TryFrom;
use crate::bindings::libevdev;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Instant {
    // Nanoseconds since some arbitrary start point.
    nsec: i128,
}

impl Instant {
    pub fn now() -> Instant {
        unsafe {
            let mut timespec: MaybeUninit<libc::timespec> = MaybeUninit::uninit();
            let result = libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut timespec as *mut _ as *mut libc::timespec);
            if result < 0 {
                panic!("Failed to determine the current time using [libc] clock_gettime(). Error code: {}",
                    std::io::Error::last_os_error().kind()
                );
            }

            timespec.assume_init().into()
        }
    }

    pub fn checked_duration_since(self, other: Instant) -> Option<Duration> {
        // The checked part referst to making sure that self is after other.
        // Panic in case of integer overflow.
        let duration = self.nsec.checked_sub(other.nsec).expect("Integer overflow while handling time.");

        Some(Duration {
            // Casting to u128 makes this function return None if other happens after self.
            nsec: u128::try_from(duration).ok()?
        })
    }
}

impl From<libc::timespec> for Instant {
    fn from(timespec: libc::timespec) -> Self {
        Self {
            nsec: NANOSECONDS_PER_SECOND * i128::from(timespec.tv_sec)
                  + i128::from(timespec.tv_nsec)
        }
    }
}

impl From<libevdev::timeval> for Instant {
    fn from(timeval: libevdev::timeval) -> Self {
        Self {
            nsec: NANOSECONDS_PER_SECOND * i128::from(timeval.tv_sec)
                  + NANOSECONDS_PER_MICROSECOND * i128::from(timeval.tv_usec)
        }
    }
}

const NANOSECONDS_PER_SECOND: i128 = 1_000_000_000;
const NANOSECONDS_PER_MICROSECOND: i128 = 1_000;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Duration {
    nsec: u128,
}

impl Duration {
    pub fn from_secs(sec: u64) -> Duration {
        Duration::from_nanos(sec * 1_000_000_000)
    }

    pub fn from_millis(msec: u64) -> Duration {
        Duration::from_nanos(msec * 1_000_000)
    }

    pub fn from_micros(microsec: u64) -> Duration {
        Duration::from_nanos(microsec * 1_000)
    }

    pub fn from_nanos(nsec: u64) -> Duration {
        Duration { nsec: nsec.into() }
    }

    pub fn as_millis(self) -> u128 {
        self.nsec / 1_000_000
    }
}

impl std::ops::Add<Duration> for Instant {
    type Output = Instant;
    fn add(self, rhs: Duration) -> Self::Output {
        let nsec = self.nsec + i128::try_from(rhs.nsec).unwrap();
        Instant { nsec }
    }
}
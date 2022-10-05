// SPDX-License-Identifier: GPL-2.0-or-later

//! A reimplementation of the standard library's time module that is guaranteed to correspond
//! to clock_gettime (Monotonic Clock). Although the standard library corresponds to that
//! as well, the documentation says that it may change over time, therefore we need our own
//! time module.

use std::mem::MaybeUninit;
use std::convert::TryInto;
use crate::bindings::libevdev;

const NANOSECONDS_PER_SECOND: i64 = 1_000_000_000;
const NANOSECONDS_PER_MICROSECOND: i64 = 1_000;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub struct Instant {
    // Second and nanoseconds since some arbitrary start point.
    sec: i64,
    // Expects invariant: 0 <= nsec < NANOSECONDS_PER_SECOND
    nsec: i64,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Duration {
    // Expects invariant: 0 <= sec
    sec: u64,
    // Expects invariant: 0 <= nsec < NANOSECONDS_PER_SECOND
    nsec: u64,
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

    // The checked part referst to making sure that self is after other.
    // Panic in case of integer overflow.
    pub fn checked_duration_since(self, other: Instant) -> Option<Duration> {
        let mut nsec = self.nsec - other.nsec;
        let mut sec = self.sec - other.sec;
        if nsec < 0 {
            nsec += NANOSECONDS_PER_SECOND;
            sec -= 1;
        }

        Some(Duration {
            sec: sec.try_into().ok()?,
            nsec: nsec.try_into().ok()?
        })
    }
}

impl From<libc::timespec> for Instant {
    fn from(timespec: libc::timespec) -> Self {
        Self {
            sec: timespec.tv_sec,
            nsec: timespec.tv_nsec,
        }
    }
}

impl From<libevdev::timeval> for Instant {
    fn from(timeval: libevdev::timeval) -> Self {
        Self {
            sec: timeval.tv_sec,
            nsec: NANOSECONDS_PER_MICROSECOND * timeval.tv_usec,
        }
    }
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
        Duration {
            sec: nsec / NANOSECONDS_PER_SECOND as u64,
            nsec: nsec % NANOSECONDS_PER_SECOND as u64
        }
    }

    pub fn as_millis(self) -> u64 {
        self.sec * 1_000 + self.nsec / 1_000_000
    }
}

// TODO: Should we prevent the user from entering ridiculously large time values in attempt to cause
// integer overflow?
impl std::ops::Add<Duration> for Instant {
    type Output = Instant;
    fn add(self, rhs: Duration) -> Self::Output {
        let mut sec = self.sec + rhs.sec as i64;
        let mut nsec = self.nsec + rhs.nsec as i64;
        if nsec > NANOSECONDS_PER_SECOND {
            nsec -= NANOSECONDS_PER_SECOND;
            sec += 1;
        }
        
        Instant { sec, nsec }
    }
}

#[test]
fn unittest() {
    let now = Instant::now();

    assert_eq!(
        now + Duration::from_secs(3),
        now + Duration::from_millis(500) + Duration::from_micros(1_700_000) + Duration::from_nanos(800_000_000)
    );
    assert_eq!(
        (now + Duration::from_secs(3)).checked_duration_since(now),
        Some(Duration::from_millis(3000))
    );
    assert_eq!(
        now.checked_duration_since(now + Duration::from_secs(3)),
        None
    );
}
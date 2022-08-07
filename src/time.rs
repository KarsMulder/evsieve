// SPDX-License-Identifier: GPL-2.0-or-later

//! A reimplementation of the standard library's time module that is guaranteed to correspond
//! to clock_gettime (Monotonic Clock). Although the standard library corresponds to that
//! as well, the documentation says that it may change over time, therefore we need our own
//! time module.

use std::mem::MaybeUninit;
use std::convert::TryFrom;
use std::cmp::Ordering;

pub struct Instant {
    timespec: libc::timespec,
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
}

impl From<libc::timespec> for Instant {
    fn from(timespec: libc::timespec) -> Self {
        Self { timespec }
    }
}

impl std::cmp::PartialEq for Instant {
    fn eq(&self, other: &Self) -> bool {
        self.timespec.tv_sec == other.timespec.tv_sec
            && self.timespec.tv_nsec == other.timespec.tv_sec
    }
}
impl std::cmp::Eq for Instant {}

impl std::cmp::PartialOrd for Instant {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl std::cmp::Ord for Instant {
    fn cmp(&self, other: &Self) -> Ordering {
        self.timespec.tv_sec.cmp(&other.timespec.tv_sec)
            .then(self.timespec.tv_nsec.cmp(&other.timespec.tv_nsec))
    }
}

const NANOSECONDS_PER_SECOND_I64: i64 = 1_000_000_000;
const NANOSECONDS_PER_SECOND_U64: u64 = 1_000_000_000;

pub struct Duration {
    nsec: u64,
}

impl Duration {
    pub fn from_nanos(nsec: u64) -> Duration {
        Duration { nsec }
    }
}

impl std::ops::Add<Duration> for Instant {
    type Output = Instant;
    fn add(self, rhs: Duration) -> Self::Output {
        let dur_nsec = rhs.nsec % NANOSECONDS_PER_SECOND_U64;
        let dur_sec = rhs.nsec / NANOSECONDS_PER_SECOND_U64;

        // Unwrap okay due to the division/modulo operators above.
        let mut sum_nsec = self.timespec.tv_nsec + i64::try_from(dur_nsec).unwrap();
        let mut sum_sec = self.timespec.tv_sec + i64::try_from(dur_sec).unwrap();

        sum_sec += sum_nsec / NANOSECONDS_PER_SECOND_I64; // Floor division
        sum_nsec %= NANOSECONDS_PER_SECOND_I64;
        
        libc::timespec {
            tv_sec: sum_sec,
            tv_nsec: sum_nsec
        }.into()
    }
}
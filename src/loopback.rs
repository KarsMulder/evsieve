// SPDX-License-Identifier: GPL-2.0-or-later

use std::time::{Instant, Duration};
use std::num::NonZeroI32;
use std::convert::TryInto;

/// Whenever a wakeup is scheduled, you get a `Token` back. At the desired time, a wakeup()
/// call with the provided token shall be made.
#[derive(PartialEq, Eq)]
pub struct Token(u64);

impl Token {
    // The `clone()` implementation is private to avoid some errors that can happen from
    // carelessly cloning tokens.
    fn clone(&self) -> Token {
        Token(self.0)
    }
}

pub struct Loopback {
    schedule: Vec<(Instant, Token)>,

    /// A counter for the amount of `Token`s that were handed out. Ensures that all handed
    /// out tokens shall be unique except in case of integer overflow.
    token_index: u64,
}

/// A LoopbackHandle contains a reference to the Loopback device, plus a virtual moment that
/// is considered to be "now". A LoopbackHandle can be used to schedule a callback after some
/// time period in the future. The instant that the callback happens shall always be computed
/// relative to the virtual "now" instant, rather than the real now instand.
/// 
/// The virtual now instant may be different from the real now instant. The virtual now may be
/// older than the real now in case there is some kind of backlog, e.g. if a callback F was
/// supposed to be handled at time X, it is now time X+5, and handling the F causes another
/// callback G to be scheduled in 2 time units, then the callback G is scheduled at X+2 rather
/// than X+7. This helps to make sure that the A key always reaches the output before the B key
/// in cases like:
/// 
///     --map key:a key:a key:b
///     --delay key:a period=0.0005
///     --delay key:a period=0.0003
///     --delay key:b period=0.001
///     --output
/// 
pub struct LoopbackHandle<'a> {
    loopback: &'a mut Loopback,
    /// If Some, then we shall emulate the current time being a certain moment in time, even
    /// if it isn't that time right now. If it is None, then it represents the actual time
    /// of the current moment, but it has not been computed yet because that would cost a
    /// syscall and we're not actually sure if we'll actually need it, and it must be computed
    /// when we actually need it.
    now: Option<Instant>,
}

pub enum Delay {
    Now,
    Never,
    /// Wait a specified amount of milliseconds.
    Wait(NonZeroI32),
}

impl Loopback {
    pub fn new() -> Loopback {
        Loopback {
            schedule: Vec::new(),
            token_index: 0,
        }
    }

    pub fn time_until_next_wakeup(&self) -> Delay {
        let next_instant_opt = self.schedule.iter()
            .map(|(instant, _token)| instant).min();
        
        // If None, then then there are no events scheduled to happen.
        let next_instant = match next_instant_opt {
            Some(value) => value,
            None => return Delay::Never,
        };

        // If None, then the event should've been scheduled at some time in the past.
        let duration = match next_instant.checked_duration_since(Instant::now()) {
            Some(value) => value,
            None => return Delay::Now,
        };

        // If Err, then the delay is very, very far in the future. It probably means the user
        // entered some bogus number for the delay. Let's not panic.
        let millisecond_wait: i32 = match duration.as_millis().try_into() {
            Ok(value) => value,
            Err(_) => return Delay::Never,
        };

        // Ensure that we do not construct a NextEventDelay::Wait(0) result.
        match NonZeroI32::new(millisecond_wait) {
            Some(value) => Delay::Wait(value),
            None => Delay::Now,
        }
    }

    /// The most overdue token that is due or overdue and removes it from self's schedule.
    /// If two due tokens are due at the exact same time, returns them in the order they
    /// were added to the Loopback device.
    /// 
    /// The reason this returns only one token is because it is possible that while processing
    /// that one token, new tokens get added to the schedule that are due before any other tokens
    /// that are actually due already. The new token should then be handled first, and that is
    /// not possible if this function were to return multiple tokens at once.
    pub fn poll_once(&mut self) -> Option<(Instant, Token)> {
        let mut ready_tokens: Vec<(Instant, Token)> = Vec::new();
        let mut remaining_schedule: Vec<(Instant, Token)> = Vec::new();
        let now = Instant::now();

        for (instant, token) in std::mem::take(&mut self.schedule) {
            if instant <= now {
                ready_tokens.push((instant, token));
            } else {
                remaining_schedule.push((instant, token));
            }
        }
        // Stably sort: make sure that the most overdue token is yielded first. Tokens that
        // are due at the exact same time should be yielded in the order they were added.
        ready_tokens.sort_by_key(|(time, _token)| *time);

        // Take the first ready token, add the rest back to the schedule.
        let mut ready_tokens_iter = ready_tokens.into_iter();
        let first_token = ready_tokens_iter.next();
        self.schedule = ready_tokens_iter.chain(remaining_schedule).collect();

        first_token
    }

    fn generate_token(&mut self) -> Token {
        if cfg!(debug_assertions) {
            self.token_index += 1;
        } else {
            self.token_index = self.token_index.wrapping_add(1);
        }
        Token(self.token_index)
    }

    pub fn get_handle(&mut self, now: Instant) -> LoopbackHandle {
        LoopbackHandle {
            loopback: self,
            now: Some(now),
        }
    }

    pub fn get_handle_lazy(&mut self) -> LoopbackHandle {
        LoopbackHandle {
            loopback: self,
            now: None,
        }
    }
}

impl<'a> LoopbackHandle<'a> {
    fn schedule_wakeup_at(&mut self, time: Instant) -> Token {
        let token = self.loopback.generate_token();
        self.loopback.schedule.push((time, token.clone()));
        token
    }

    pub fn schedule_wakeup_in(&mut self, delay: Duration) -> Token {
        let now = self.now();
        self.schedule_wakeup_at(now + delay)
    }

    /// If a previously-scheduled wakeup no longer seems needed, you can cancel it to save some
    /// CPU cycles later.
    pub fn cancel_token(&mut self, token: Token) {
        self.loopback.schedule.retain(|(_, other_token)| token != *other_token);
    }

    /// Like self.now, but lazily computes the current time if it wasn't already stored
    /// in self.now.
    fn now(&mut self) -> Instant {
        let time = match self.now {
            Some(time) => time,
            None => Instant::now(),
        };
        self.now = Some(time);
        time
    }
}
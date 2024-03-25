use std::{
    boxed::Box,
    collections::VecDeque,
    ops::{Add, Sub},
    sync::{LazyLock, Mutex},
};

use libc::c_long;

use crate::{lock_step::LOCK_STEP_CURRENT_TIME, pthread::* };

pub static HRT_QUEUE: LazyLock<Box<HRTQueue>> = LazyLock::new(|| {
    let m = HRTQueue::new();
    m
});

#[derive(PartialEq, Clone, Copy, Debug)]
pub struct Timespec {
    pub sec: c_long,
    pub nsec: c_long,
}

impl PartialOrd for Timespec {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        let sub = Self {
            sec: self.sec - other.sec,
            nsec: self.nsec - other.nsec,
        };

        (sub.sec * 1000 * 1000 * 1000 + sub.nsec).partial_cmp(&0)
    }
}

impl From<libc::timespec> for Timespec {
    fn from(value: libc::timespec) -> Self {
        Self {
            sec: value.tv_sec,
            nsec: value.tv_nsec,
        }
    }
}

impl Sub for Timespec {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self {
        Self {
            sec: self.sec - rhs.sec,
            nsec: self.nsec - rhs.nsec,
        }
    }
}

impl Sub<libc::timespec> for Timespec {
    type Output = Timespec;

    fn sub(self, rhs: libc::timespec) -> Self::Output {
        Self {
            sec: self.sec - rhs.tv_sec,
            nsec: self.nsec - rhs.tv_nsec,
        }
    }
}

impl Add<c_long> for Timespec {
    type Output = Self;
    fn add(self, rhs: c_long) -> Self::Output {
        Self {
            sec: self.sec,
            nsec: self.nsec + rhs,
        }
    }
}

impl Add for Timespec {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        let ret = Self {
            sec: self.sec + rhs.sec,
            nsec: self.nsec + rhs.nsec,
        };
        ret
    }
}

impl Timespec {
    pub fn to_nano(&self) -> i64 {
        self.sec as i64 * 1000 * 1000 * 1000 + self.nsec as i64
    }

    pub fn from_secs(sec: i64) -> Self {
        Self { sec:sec as c_long, nsec: 0 }
    }
}

pub fn get_time_now() -> Timespec {
    let mut tp = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };

    if cfg!(feature = "lock_step_enabled") {
        *LOCK_STEP_CURRENT_TIME.lock().unwrap()
    } else {
        unsafe {
            libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut tp as *mut libc::timespec);
        }
        Timespec::from(tp)
    }
}

pub struct HRTEntry {
    pub deadline: Timespec,
    pub callback: Box<dyn Fn() + 'static>,
}
unsafe impl Send for HRTEntry {}
unsafe impl Sync for HRTEntry {}

impl HRTEntry {

    pub fn new<F>(deadline: Timespec, callback: F) -> HRTEntry
    where
        F: Fn() + 'static,
    {
        HRTEntry {
            deadline,
            callback: Box::new(callback),
        }
    }
}
pub struct HRTQueue {
    list: Mutex<VecDeque<HRTEntry>>,
    thread_id: libc::pthread_t,
}

const DURATION_1_MS: c_long = 1000 * 1000;

#[allow(unreachable_code)]
extern "C" fn hrtqueue_run(ptr: *mut libc::c_void) -> *mut libc::c_void {
    let htr_queue = unsafe { &*(ptr as *mut HRTQueue) };

    let mut sleep_time = DURATION_1_MS; // default to sleep 1ms

    loop {
        loop {
            let mut unlock_list = htr_queue.list.lock().unwrap();
            if let Some(x) = unlock_list.front() {
                let now = get_time_now();
                if now >= x.deadline {
                    (x.callback)();
                    unlock_list.pop_front();
                } else {
                    let escaped = (x.deadline - now).to_nano();
                    sleep_time = if escaped > DURATION_1_MS as i64 {
                        DURATION_1_MS
                    } else {
                        escaped as c_long
                    };
                    break;
                }
            } else {
                break; // the list is empty
            }
        }

        // lock is released here, so other thread could do some adding
        nanosleep(sleep_time);
    }
    std::ptr::null_mut()
}

extern "C" fn null_signal_handler(_: i32) {}

impl HRTQueue {
    fn new() -> Box<Self> {
        let mut queue = Box::new(HRTQueue {
            list: Mutex::new(VecDeque::new()),
            thread_id: 0,
        });

        let queue_ptr = &mut *queue as *mut HRTQueue as *mut libc::c_void;
        let fifo_scheduled;
        if cfg!(test) {
            fifo_scheduled = false;
        } else {
            fifo_scheduled = true;
        }

        let _thread_id = create_phtread(16384, 99, hrtqueue_run, queue_ptr, fifo_scheduled);
        queue.thread_id = _thread_id;

        unsafe {
            libc::signal(libc::SIGCONT, null_signal_handler as libc::sighandler_t);
        }
        queue
    }

    #[inline]
    fn awake(&self) {
        unsafe {
            libc::pthread_kill(self.thread_id, libc::SIGCONT);
        }
    }

    pub fn add(&self, entry: HRTEntry) {
        let mut unlock_list = self.list.lock().unwrap();

        if unlock_list.is_empty() || unlock_list.front().unwrap().deadline > entry.deadline {
            unlock_list.push_front(entry);
            self.awake();
        } else {
            let may_be_index = unlock_list
                .iter_mut()
                .position(|x| x.deadline > entry.deadline);
            match may_be_index {
                Some(index) => unlock_list.insert(index, entry),
                None => unlock_list.push_back(entry),
            };
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    #[test]
    fn test_awake() {
        let queue = HRTQueue::new();
        queue.awake();
        Box::leak(queue);
    }

}

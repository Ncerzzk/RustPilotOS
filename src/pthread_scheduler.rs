use std::{
    cell::Cell, ptr::null_mut, sync::Arc
};

use libc::{c_long, c_ulong};

use crate::{
    hrt::{get_time_now, Timespec},
    pthread::{create_phtread, nanosleep},
};

thread_local! {
    static LAST_SCHEDULED_TIME:Cell<Timespec> = Cell::new(Timespec{sec: 0, nsec:0});
    static DEADLINE:Cell<Timespec> = Cell::new(Timespec{sec: 0, nsec:0});
}

pub struct SchedulePthread {
    thread_func: fn(*mut libc::c_void) -> *mut libc::c_void,
    pub thread_args: *mut libc::c_void,
    pub thread_id: c_ulong,
}

impl SchedulePthread {
    extern "C" fn wrapper(ptr: *mut libc::c_void) -> *mut libc::c_void {
        let sp = unsafe { Arc::from_raw(ptr as *const SchedulePthread) };
        
        (sp.thread_func)(Arc::into_raw(sp) as *mut libc::c_void);
        null_mut()
    }

    fn simple_wrapper(ptr: *mut libc::c_void) -> *mut libc::c_void {
        let sp = unsafe { Arc::from_raw(ptr as *const SchedulePthread) };

        let b = sp.thread_args as *mut Box<dyn FnOnce(Arc<SchedulePthread>)>;
        let a = unsafe { Box::from_raw(b) };
        (a)(sp);
        null_mut()
    }

    pub fn new_simple(f: Box<dyn FnOnce(Arc<SchedulePthread>)>) -> Arc<Self> {
        let func = Box::new(f);

        let a = Box::into_raw(func) as *mut libc::c_void;

        Self::new(1024 * 1024, 50, Self::simple_wrapper, a, false)
    }

    pub fn new_fifo(
        stack_size: u32,
        priority: i32,
        f: Box<dyn FnOnce(Arc<SchedulePthread>)>,
    ) -> Arc<Self> {
        let func = Box::new(f);

        let a = Box::into_raw(func) as *mut libc::c_void; 
        Self::new(stack_size, priority, Self::simple_wrapper, a, true)
    }

    pub fn new(
        stack_size: u32,
        priority: i32,
        f: fn(*mut libc::c_void) -> *mut libc::c_void,
        extral_args: *mut libc::c_void,
        is_fifo_schedule: bool,
    ) -> Arc<Self> {
        let ret = Arc::new(SchedulePthread {
            thread_func: f,
            thread_args: extral_args,
            thread_id: 0
        });
        let id = create_phtread(
            stack_size,
            priority,
            Self::wrapper,
            Arc::into_raw(ret.clone()) as *mut libc::c_void,
            is_fifo_schedule,
        );
        unsafe {
            (*(Arc::as_ptr(&ret) as *mut SchedulePthread)).thread_id = id;
        }
        ret
    }

    pub fn join(&self) {
        unsafe {
            libc::pthread_join(self.thread_id, null_mut());
        }
    }

    pub fn schedule_after(self: &Arc<Self>, us: c_long) {
        DEADLINE.set(get_time_now() + us * 1000);
        nanosleep(us * 1000);
        LAST_SCHEDULED_TIME.set(get_time_now());
    }

    pub fn schedule_until(self: &Arc<Self>, us: c_long) {
        let deadline = LAST_SCHEDULED_TIME.get() + us * 1000;
        DEADLINE.set(deadline);

        let now = get_time_now();
        nanosleep((deadline - now).to_nano() as c_long);
        LAST_SCHEDULED_TIME.set(get_time_now());
    }
}

#[cfg(test)]
mod tests {
    use std::ptr::null;

    use super::*;

    #[test]
    fn test_basic_pthread_schedule() {
        fn test(ptr: *mut libc::c_void) -> *mut libc::c_void {
            let sp = unsafe { Arc::from_raw(ptr as *const SchedulePthread) };
            let num_ptr = sp.thread_args as *mut i32;

            sp.schedule_until(10 * 1000); // 10 ms
            unsafe {
                *num_ptr += 1;
            }

            sp.schedule_after(5000);

            unsafe {
                *num_ptr += 1;
            }
            null_mut()
        }

        let mut num = 0;
        let sp = SchedulePthread::new(
            16384,
            1,
            test,
            &mut num as *mut i32 as *mut libc::c_void,
            false,
            None,
        );

        nanosleep(13 * 1000 * 1000); // sleep to wait the thread start excuting
        assert_eq!(num, 1);

        nanosleep(1000 * 1000 * 6);
        assert_eq!(num, 2);
    }

    #[test]
    fn test_pthread_schedule_by_freq() {
        fn test(ptr: *mut libc::c_void) -> *mut libc::c_void {
            let sp = unsafe { Arc::from_raw(ptr as *const SchedulePthread) };
            let num_ptr = sp.thread_args as *mut i32;

            let start_time = std::time::SystemTime::now();
            while unsafe { *num_ptr } < 400 {
                unsafe {
                    *num_ptr += 1;
                }
                sp.schedule_until(2500);
            }
            let spend_time = std::time::SystemTime::elapsed(&start_time);
            println!("spend:{:?}", spend_time);

            null_mut()
        }

        let mut num = 0;
        let sp = SchedulePthread::new(
            16384,
            99,
            test,
            &mut num as *mut i32 as *mut libc::c_void,
            false,
            None,
        );

        std::thread::sleep(std::time::Duration::from_secs(2));
    }

    #[test]
    fn test_simple_thread() {
        let a = 1;

        let thread = SchedulePthread::new_simple(Box::new(move |_| {
            assert_eq!(a, 1);
        }));

        unsafe { libc::pthread_join(thread.thread_id, null_mut()) };
    }
}

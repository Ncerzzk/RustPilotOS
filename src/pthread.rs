use std::mem::MaybeUninit;

#[inline]
pub fn nanosleep(ns: i64) -> i64 {
    // ns should less than  999999999
    #[cfg(feature= "lock_step_enabled")]{
        crate::lock_step::lock_step_nanosleep(ns)
    }

    #[cfg(not(feature = "lock_step_enabled"))]{
        let t = libc::timespec {
            tv_sec: 0,
            tv_nsec: ns,
        };
        let mut rt = libc::timespec {
            tv_sec: 0,
            tv_nsec: 0,
        };
        unsafe {
            libc::nanosleep(&t as *const libc::timespec, &mut rt as *mut libc::timespec);
        };
        rt.tv_nsec
    }

}

pub fn create_phtread(
    stack_size: u32,
    priority: i32,
    f: extern "C" fn(*mut libc::c_void) -> *mut libc::c_void,
    value: *mut libc::c_void,
    is_fifo_schedule: bool,
) -> u64 {
    unsafe {
        let mut attr = MaybeUninit::<libc::pthread_attr_t>::uninit();
        let attr_ptr = attr.as_mut_ptr();
        libc::pthread_attr_init(attr_ptr);
        libc::pthread_attr_setstacksize(attr_ptr, stack_size as usize);
        libc::pthread_attr_setinheritsched(attr_ptr, libc::PTHREAD_EXPLICIT_SCHED);
        {
            if is_fifo_schedule {
                libc::pthread_attr_setschedpolicy(attr_ptr, libc::SCHED_FIFO);
                if libc::getuid() !=0{
                    panic!("please use root to create the fifo thread!");
                }
            } else {
                libc::pthread_attr_setschedpolicy(attr_ptr, libc::SCHED_OTHER);
            }
        }

        let param = libc::sched_param {
            sched_priority: priority,
        };
        libc::pthread_attr_setschedparam(attr_ptr, &param);

        let mut pthread = MaybeUninit::<libc::pthread_t>::uninit();
        libc::pthread_create(pthread.as_mut_ptr(), attr_ptr, f, value);
        pthread.assume_init()
    }
}

#[cfg(test)]
mod tests {
    use libc::{pthread_join, pthread_kill};

    use super::*;
    extern "C" fn testfunc(_: *mut libc::c_void) -> *mut libc::c_void {
        println!("hello,this is a test func!");
        return std::ptr::null_mut();
    }
    #[test]
    fn test_create_pthread() {
        create_phtread(2048, 1, testfunc, 0 as *mut libc::c_void, false);
    }

    extern "C" fn sleep_func(ret: *mut libc::c_void) -> *mut libc::c_void {
        //std::thread::sleep(std::time::Duration::from_secs(2)); the std library sleep could not be waken.
        unsafe {
            *(ret as *mut u128) = nanosleep(999999999) as u128;
        }
        println!("hello,this is a sleep func!");
        return std::ptr::null_mut();
    }

    extern "C" fn signal_handler(sig: i32) {
        println!("handler! {}\n", sig);
    }

    #[test]
    fn test_wake_sleep_pthread() {
        let mut ret: u128 = 0;
        let ptr = (&mut ret as *mut u128) as *mut libc::c_void;

        unsafe {
            // let mut mask =  MaybeUninit::<libc::sigset_t>::uninit();
            // libc::sigemptyset(mask.as_mut_ptr());

            // let act = libc::sigaction{
            //     sa_mask: mask.assume_init(),
            //     sa_flags: 0,
            //     sa_restorer: Option::None,
            //     sa_sigaction:signal_handler as libc::sighandler_t,
            // };

            // libc::sigaction(libc::SIGCONT, &act as *const libc::sigaction,std::ptr::null_mut());

            libc::signal(libc::SIGCONT, signal_handler as libc::sighandler_t);
        }
        let thread = create_phtread(2048, 1, sleep_func, ptr, false);

        nanosleep(499999999); // sleep to make sure the pthread has get into sleep
        unsafe {
            pthread_kill(thread, libc::SIGCONT);
            pthread_join(thread, std::ptr::null_mut());
        }
        println!("ret:{}", ret);
        assert_ne!(ret, 0);
    }

    #[test]
    fn test_nanosleep_ret_val() {
        assert_eq!(nanosleep(99999), 0);
    }
}

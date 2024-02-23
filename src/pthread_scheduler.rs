use std::{
    mem::{transmute, MaybeUninit},
    ptr::{null, null_mut},
    sync::{Arc, Condvar, Mutex, RwLock, Weak}, cell::RefCell,
};

use libc::{c_long, c_ulong};

use crate::{
    hrt::{get_time_now, HRTEntry, Timespec, HRT_QUEUE},
    pthread::{create_phtread, nanosleep},
};

pub struct SchedulePthread {
    condvar: Condvar,
    should_exit: Mutex<bool>,
    specific_data: *const libc::c_void,
    specific_key: Option<u32>,
    thread_func: fn(*mut libc::c_void) -> *mut libc::c_void,
    pub thread_args: *mut libc::c_void,
    pub last_scheduled_time: RwLock<Timespec>,
    pub thread_id:c_ulong,
    pub deadline:RwLock<Timespec>
}

impl SchedulePthread {
    extern "C" fn wrapper(ptr: *mut libc::c_void) -> *mut libc::c_void {
        let sp = unsafe { Arc::from_raw(ptr as *const SchedulePthread) };
        if let Some(key) = sp.specific_key {
            unsafe {
                libc::pthread_setspecific(key, sp.specific_data);
            }
        };
        (sp.thread_func)(Arc::into_raw(sp) as *mut libc::c_void);
        null_mut()
    }

    
    fn simple_wrapper(ptr:*mut libc::c_void) -> *mut libc::c_void{
        let sp = unsafe { Arc::from_raw(ptr as *const SchedulePthread) };

        let b = sp.thread_args as *mut Box<dyn FnOnce()>;
        let a = unsafe { Box::from_raw(b) };
        (a)();
        null_mut()
    }

    pub fn new_simple(f: Box<dyn FnOnce()>) ->Arc<Self>
        {
            let func = Box::new(f);

            let a = Box::into_raw(func) as *mut libc::c_void;

            Self::new(8192, 50, Self::simple_wrapper, a, false, None)
        }


    pub fn new(
        stack_size: u32,
        priority: i32,
        f: fn(*mut libc::c_void) -> *mut libc::c_void,
        extral_args: *mut libc::c_void,
        is_fifo_schedule: bool,
        pthread_key: Option<u32>,
    ) -> Arc<Self> {
        let spec_data;
        if let Some(pthread_key) = pthread_key {
            unsafe {
                spec_data = libc::pthread_getspecific(pthread_key);
            }
        } else {
            spec_data = null_mut();
        }

        let ret = Arc::new(SchedulePthread {
            condvar: Condvar::new(),
            should_exit: Mutex::new(false),
            specific_data: spec_data,
            specific_key: pthread_key,
            thread_func: f,
            thread_args: extral_args,
            last_scheduled_time: RwLock::new(get_time_now()),
            thread_id:0,
            deadline:RwLock::new(get_time_now())
        });
        let id = create_phtread(
            stack_size,
            priority,
            Self::wrapper,
            Arc::into_raw(ret.clone()) as *mut libc::c_void,
            is_fifo_schedule,
        );
        unsafe{
            (*(Arc::as_ptr(&ret) as *mut SchedulePthread)).thread_id = id;
        }
        ret
    }

    pub fn join(&self){
        unsafe{
            libc::pthread_join(self.thread_id,null_mut());
        }
    }

    fn wake_schedule_pthread(sp: &SchedulePthread) {
        let mut a = sp.last_scheduled_time.write().unwrap();
        *a = get_time_now();
        sp.condvar.notify_all();
    }

    pub fn schedule_after(self: &Arc<Self>, us: c_long) {
        // let p = self.clone();
        // let deadline = get_time_now() + us * 1000;
        // let entry = HRTEntry::new(deadline, move || {
        //     Self::wake_schedule_pthread(p.as_ref());
        // });
        // *(self.deadline.write().unwrap()) = deadline;
        // HRT_QUEUE.add(entry);
        // drop(self.condvar.wait(self.should_exit.lock().unwrap()));
        // lock is released here, so other thread could do some adding
        *(self.deadline.write().unwrap()) = get_time_now() + us * 1000;
        nanosleep(us * 1000);
        *(self.last_scheduled_time.write().unwrap()) = get_time_now();
    }

    pub fn schedule_until(self: &Arc<Self>, us: c_long) {
        let p = self.clone();
        let deadline = *(self.last_scheduled_time.read().unwrap()) + us * 1000;
        *(self.deadline.write().unwrap()) = deadline;

        let now = get_time_now();
        nanosleep((deadline - now).to_nano() as c_long);
        *(self.last_scheduled_time.write().unwrap()) = get_time_now();
        // let entry = HRTEntry::new(
        //     deadline,
        //     move || {
        //         Self::wake_schedule_pthread(p.as_ref());
        //     },
        // );
        // *(self.deadline.write().unwrap()) = deadline;
        // HRT_QUEUE.add(entry);
        // drop(self.condvar.wait(self.should_exit.lock().unwrap()));
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
            2048,
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
            2048,
            99,
            test,
            &mut num as *mut i32 as *mut libc::c_void,
            false,
            None,
        );

        std::thread::sleep(std::time::Duration::from_secs(2));
    }

    #[test]
    fn test_simple_thread(){
        let a = 1;

        let thread = SchedulePthread::new_simple(Box::new(move ||{
            assert_eq!(a,1);
        }));

        unsafe { libc::pthread_join(thread.thread_id,null_mut()) };
    }
}

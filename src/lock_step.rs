use std::{sync::{LazyLock, Mutex, Condvar, atomic::{AtomicBool, Ordering}}};

pub static LOCK_STEP_CURRENT_TIME:LazyLock<Mutex<libc::timespec>> = LazyLock::<Mutex<libc::timespec>>::new(||{
    Mutex::new(libc::timespec{tv_sec:0, tv_nsec:0})
});

pub static LOCK_STEP_EARLY_WAKEN:LazyLock<AtomicBool> = LazyLock::<AtomicBool>::new(||{
    AtomicBool::new(false)
});

pub static LOCK_STEP_CONVAR:LazyLock<Condvar> = LazyLock::<Condvar>::new(||{
    Condvar::new()
});

pub fn lock_step_update_time(new_time:libc::timespec){
    *LOCK_STEP_CURRENT_TIME.lock().unwrap() = new_time;
}

pub fn lock_step_nanosleep(ns:i64)->i64{
    let mut nsec:i64;
    let mut sec:i64;

    {
        let current = LOCK_STEP_CURRENT_TIME.lock().unwrap();
        nsec = ns + current.tv_nsec;
        sec = current.tv_sec;
    }

    if nsec > 999999999{
        sec += 1;
        nsec -= 999999999;
    }

    let deadline = libc::timespec{
        tv_sec:sec,
        tv_nsec:nsec
    };
    let ret = loop{
        let current = LOCK_STEP_CURRENT_TIME.lock().unwrap();
        if current.tv_sec >= deadline.tv_sec && current.tv_nsec > deadline.tv_nsec{
            break 0
        }else if LOCK_STEP_EARLY_WAKEN.fetch_nand(true, Ordering::SeqCst){
            break {if deadline.tv_nsec - current.tv_nsec < 0 {deadline.tv_nsec - current.tv_nsec + 999999999} else {deadline.tv_nsec - current.tv_nsec}}
        }
        println!("hello?");
        let _= LOCK_STEP_CONVAR.wait(current);
        println!("after condvar!");
    };

    println!("out!");
    ret
}

#[cfg(test)]
mod tests{
    use libc::{pthread_join, pthread_kill};

    use super::*;
    use crate::pthread::*;

    extern "C" fn sleep_func(ret:*mut libc::c_void) -> *mut libc::c_void{
        unsafe{
            *(ret as *mut u128) = lock_step_nanosleep(999999999) as u128;
        }
        println!("hello,this is a sleep func!");
        return std::ptr::null_mut();
    }

    extern "C" fn signal_handler(sig:i32){

        LOCK_STEP_EARLY_WAKEN.store(true, Ordering::SeqCst);
        LOCK_STEP_CONVAR.notify_one();
        println!("handler! {}\n",sig);
    }

    #[test]
    fn test_wake_sleep_pthread_of_lock_step(){
        let mut ret:u128 = 0;
        let ptr = (&mut ret as *mut u128) as *mut libc::c_void;

        unsafe{
            libc::signal(libc::SIGUSR1,signal_handler as libc::sighandler_t);
        }
        let thread = create_phtread(2048, 1, sleep_func, ptr,false);

        nanosleep(499999999); // sleep to make sure the pthread has get into sleep
        unsafe{
            pthread_kill(thread, libc::SIGUSR1);
            pthread_join(thread, std::ptr::null_mut());
        }
        assert_ne!(ret,0);
    }

}
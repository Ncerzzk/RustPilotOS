use std::{sync::{LazyLock, Mutex, Condvar, atomic::{AtomicBool, Ordering}, Once}};
use crate::hrt::Timespec;


pub static LOCK_STEP_CURRENT_TIME:LazyLock<Mutex<Timespec>> = LazyLock::<Mutex<Timespec>>::new(||{
    Mutex::new(Timespec{sec:0, nsec:0})
});

pub static LOCK_STEP_EARLY_WAKEN:LazyLock<AtomicBool> = LazyLock::<AtomicBool>::new(||{
    AtomicBool::new(false)
});

pub static LOCK_STEP_CONVAR:LazyLock<Condvar> = LazyLock::<Condvar>::new(||{
    Condvar::new()
});

pub fn lock_step_update_time(new_time:Timespec){
    *LOCK_STEP_CURRENT_TIME.lock().unwrap() = new_time;
}

static INIT_TEST_THREAD:Once = Once::new();
pub fn lock_step_nanosleep(ns:i64)->i64{
    let mut nsec:i64;
    let mut sec:i64;

    #[cfg(all(test,feature="lock_step_enabled"))]
    INIT_TEST_THREAD.call_once(||{
        std::thread::spawn(||{
            loop {
                let time = std::time::SystemTime::now();
                let dur = time.duration_since(std::time::SystemTime::UNIX_EPOCH).unwrap();
                let time_spec = Timespec{
                    tv_sec: dur.as_secs() as i64,
                    tv_nsec: (dur.as_nanos() % 999999999 ) as i64
                };
                lock_step_update_time(time_spec);
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
        });
        std::thread::sleep(std::time::Duration::from_secs(1));
    });

    let mut current;
    {
        current = *LOCK_STEP_CURRENT_TIME.lock().unwrap();
    }

    let deadline = current + ns;

    let ret = loop{
        let current_guard: std::sync::MutexGuard<'_, Timespec> = LOCK_STEP_CURRENT_TIME.lock().unwrap();
        let current = *current_guard;
        if current >= deadline{
            break 0
        }else if LOCK_STEP_EARLY_WAKEN.fetch_nand(true, Ordering::SeqCst){
            break { (deadline - current).to_nano() }
        }
        println!("hello?");
        let _= LOCK_STEP_CONVAR.wait(current_guard);
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
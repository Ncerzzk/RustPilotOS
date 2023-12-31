use std::{sync::{Condvar, Mutex, Arc, Weak, RwLock}, mem::{MaybeUninit, transmute}, ptr::{null_mut, null}};

use crate::{pthread::{create_phtread, nanosleep}, hrt::{HRTEntry, get_time_now, HRT_QUEUE, Timespec}};

pub struct SchedulePthread{
    condvar:Condvar,
    should_exit:Mutex<bool>,
    specific_data:*const libc::c_void,
    specific_key:Option<u32>,
    thread_func:fn(*mut libc::c_void) -> *mut libc::c_void,
    pub thread_args:*mut libc::c_void,
    last_scheduled_time:RwLock<Timespec>
}

impl SchedulePthread{
    extern "C" fn wrapper(ptr:*mut libc::c_void)-> *mut libc::c_void{
        let sp = unsafe{Arc::from_raw(ptr as *const SchedulePthread)};
        if let Some(key) = sp.specific_key{
            unsafe{
                libc::pthread_setspecific(key, sp.specific_data);
            }
        };
        (sp.thread_func)(Arc::into_raw(sp) as *mut libc::c_void);
        null_mut()
    }


    pub fn new(stack_size:u32, priority:i32, f:fn(*mut libc::c_void) -> *mut libc::c_void, extral_args:*mut libc::c_void, is_fifo_schedule:bool,pthread_key:Option<u32>)->Arc<Self>{
        let spec_data;
        if let Some(pthread_key) = pthread_key{
            unsafe{
                spec_data = libc::pthread_getspecific(pthread_key);
            }
        }else{
            spec_data = null_mut();
        }

        let mut ret = Arc::new(
            SchedulePthread { 
                condvar:Condvar::new(),
                should_exit:Mutex::new(false),
                specific_data:spec_data,
                specific_key:pthread_key,
                thread_func:f,
                thread_args:extral_args,
                last_scheduled_time:RwLock::new(get_time_now())
            }
        );
        create_phtread(stack_size, priority, Self::wrapper, Arc::into_raw(ret.clone()) as *mut libc::c_void, is_fifo_schedule);
        ret
    }

    fn wake_schedule_pthread(sp:&SchedulePthread){
        sp.condvar.notify_all();
        let mut a = sp.last_scheduled_time.write().unwrap();
        *a = get_time_now();
    }

    pub fn schedule_after(self:&Arc<Self>,us:i64){
        let p = self.clone();
        let entry = HRTEntry::new(get_time_now() + us * 1000, move||{
            Self::wake_schedule_pthread(p.as_ref());
        });
        HRT_QUEUE.add(entry);
        let _ = self.condvar.wait(self.should_exit.lock().unwrap());
    }

    pub fn schedule_until(self:&Arc<Self>,us:i64){
        let p = self.clone();
        let entry = HRTEntry::new(*(self.last_scheduled_time.read().unwrap()) + us * 1000, move||{
            Self::wake_schedule_pthread(p.as_ref());
        });
        HRT_QUEUE.add(entry);
        let _ = self.condvar.wait(self.should_exit.lock().unwrap());
    }

     
}


#[cfg(test)]
mod tests{
    use std::ptr::null;

    use super::*;



    #[test]
    fn test_basic_pthread_schedule(){
        fn test(ptr:*mut libc::c_void)->*mut libc::c_void{
            let sp = unsafe{Arc::from_raw(ptr as *const SchedulePthread)};
            let num_ptr = sp.thread_args as *mut i32;
    
            sp.schedule_until(10 * 1000);   // 10 ms
            unsafe{
                *num_ptr +=1;
            }
    
            sp.schedule_after(5000);
    
            unsafe{
                *num_ptr +=1;
            }
            null_mut()
        }

        let mut num = 0;
        let sp = SchedulePthread::new(
            2048,1,test,&mut num as *mut i32 as *mut libc::c_void, false,None);
        
        nanosleep(13*1000*1000); // sleep to wait the thread start excuting
        assert_eq!(num,1);

        nanosleep(1000*1000*6);
        assert_eq!(num,2);
    }

    #[test]
    fn test_pthread_schedule_by_freq(){
        fn test(ptr:*mut libc::c_void) ->*mut libc::c_void{
            let sp = unsafe{Arc::from_raw(ptr as *const SchedulePthread)};
            let num_ptr = sp.thread_args as *mut i32;
            
            let start_time = std::time::SystemTime::now();
            while unsafe{*num_ptr} < 400{
                unsafe{*num_ptr +=1;}
                sp.schedule_until(2500);
            }
            let spend_time = std::time::SystemTime::elapsed(&start_time);
            println!("spend:{:?}",spend_time);

            null_mut()
        }

        let mut num = 0;
        let sp = SchedulePthread::new(
            2048,99,test,&mut num as *mut i32 as *mut libc::c_void, false,None);

        std::thread::sleep(std::time::Duration::from_secs(2));
    }
}
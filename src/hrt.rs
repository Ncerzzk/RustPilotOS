use std::{sync::{Mutex, Arc,LazyLock},boxed::Box, collections::VecDeque, time::{Duration, SystemTime}};

use crate::{workqueue::*, pthread::*};

#[cfg(feature="lock_step_enabled")]
use crate::{lock_step::lock_step_nanosleep};

pub static HRT_QUEUE: LazyLock<Box<HRTQueue>> = LazyLock::new(|| {
    let m = HRTQueue::new();
    m
});

pub struct HRTEntry{
    pub deadline:SystemTime,
    pub workitem:Arc<WorkItem>
}

pub struct HRTQueue{
    list:Mutex<VecDeque<HRTEntry>>,
    thread_id:libc::pthread_t
}

const DURATION_1_MS:Duration = Duration::from_millis(1);

#[allow(unreachable_code)] 
extern "C" fn hrtqueue_run(ptr:*mut libc::c_void)-> *mut libc::c_void{
    let htr_queue = unsafe{&*(ptr as *mut HRTQueue)};

    let mut sleep_time = DURATION_1_MS;   // default to sleep 1ms

    loop{
        loop{
            let mut unlock_list = htr_queue.list.lock().unwrap();
            if let Some(x) = unlock_list.front(){
                let now = SystemTime::now();
                if now >= x.deadline{
                    x.workitem.schedule();
                    unlock_list.pop_front();
                }else{
                    let escaped = x.deadline.duration_since(now).unwrap();
                    sleep_time = if escaped >  DURATION_1_MS { DURATION_1_MS } else {escaped};
                    break;
                }
            }else{
                break; // the list is empty
            }
        }

        // lock is released here, so other thread could do some adding
        #[cfg(not(feature="lock_step_enabled"))]
        nanosleep(sleep_time.as_nanos() as i64);

        #[cfg(feature="lock_step_enabled")]
        lock_step_nanosleep(sleep_time.as_nanos() as i64);
    } 
    std::ptr::null_mut()
}


extern "C" fn null_signal_handler(_:i32){}

impl HRTQueue{
    fn new()->Box<Self>{
        let mut queue = Box::new(HRTQueue{
            list: Mutex::new(VecDeque::new()),
            thread_id: 0
        });

        let queue_ptr = &mut *queue  as *mut HRTQueue as *mut libc::c_void;
        let fifo_scheduled;
        if cfg!(test){
            fifo_scheduled = false;
        }else{
            fifo_scheduled = true;
        }

        let _thread_id = create_phtread(2048, 99, hrtqueue_run, queue_ptr,fifo_scheduled);
        queue.thread_id = _thread_id;
        
        unsafe{
            libc::signal(libc::SIGCONT,null_signal_handler as libc::sighandler_t);
        }
        queue
    }

    #[inline]
    fn awake(&self){
        unsafe{
            libc::pthread_kill(self.thread_id, libc::SIGCONT);
        }
    }

    pub fn add(&self,entry:HRTEntry){
        let mut unlock_list = self.list.lock().unwrap();

        if unlock_list.is_empty() || unlock_list.front().unwrap().deadline > entry.deadline{
            unlock_list.push_front(entry);
            self.awake();
        }else{
            let may_be_index = unlock_list.iter_mut().position(|x| x.deadline > entry.deadline);
            match may_be_index{
                Some(index) => unlock_list.insert(index, entry),
                None => unlock_list.push_back(entry)
            };
        }
    }
}

#[cfg(test)]
mod tests{
    use super::*;
    use crate::workqueue::tests::GPS;

    #[test]
    fn test_awake(){
        let queue = HRTQueue::new();
        queue.awake();
        Box::leak(queue);
    }

    #[test]
    fn test_hrt_basic(){
        let queue = Box::leak(HRTQueue::new());
        let wq = WorkQueue::new(2048,1,false);
        let gps = GPS::new(&wq);

        let entry = HRTEntry{
            deadline: SystemTime::now(),
            workitem: gps.item.clone()
        };
        queue.add(entry);

        while !gps.finish{};

        let ptr = Arc::as_ptr(&gps) as *mut GPS;
        unsafe{
            (*ptr).finish = false;
        }
        assert_eq!(gps.finish,false);

        let entry = HRTEntry{
            deadline: SystemTime::now() + Duration::from_secs(5000),
            workitem: gps.item.clone() 
        };

        queue.add(entry);

        std::thread::sleep(Duration::from_secs(5));

        assert_eq!(gps.finish,false);
    }

    #[test]
    fn test_multi_works_order(){
        let queue = Box::leak(HRTQueue::new());
        let wq = WorkQueue::new(2048,1,false); 

        let gps1 = GPS::new(&wq);
        let gps2 = GPS::new(&wq);

        queue.add(HRTEntry { deadline: SystemTime::now() + Duration::from_secs(3), workitem: gps1.item.clone() });
        queue.add(HRTEntry { deadline: SystemTime::now() + Duration::from_secs(5), workitem: gps2.item.clone() });

        std::thread::sleep(Duration::from_secs(1));
        assert_eq!(gps1.finish,false);
        assert_eq!(gps2.finish,false);
        std::thread::sleep(Duration::from_secs(3));
        assert_eq!(gps1.finish,true); 
        std::thread::sleep(Duration::from_secs(2)); 
        assert_eq!(gps2.finish,true); 
    }
}
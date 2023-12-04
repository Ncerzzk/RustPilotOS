#![feature(new_uninit)]
use std::{sync::{Mutex, Condvar, Arc, Weak}, collections::VecDeque, thread::{spawn}, borrow::{Borrow, BorrowMut}, ptr::null_mut, any::Any, mem::MaybeUninit, pin::*, time::{SystemTime, Duration}};
use crate::pthread::create_phtread;
use crate::hrt::{HRTEntry,HRT_QUEUE};
use std::boxed::Box;


pub trait Callable {
    fn call(&mut self);
}

pub struct WorkItem{
    queue:Weak<WorkQueue>,
    func:fn(* mut libc::c_void),
    parent:* mut dyn Any,
    last_call_time:SystemTime
}

unsafe impl Send for WorkItem{}
unsafe impl Sync for WorkItem{}
impl WorkItem{
    pub fn new<'a>(wq:&Arc<WorkQueue>,parent:* mut dyn Any, func:fn(* mut libc::c_void)) -> Arc<WorkItem>{
        Arc::new(
            WorkItem{
                queue:Arc::downgrade(wq),
                func,
                parent,
                last_call_time:SystemTime::now()
            }
        )
    }

    pub fn schedule(self:&Arc<Self>){  
        self.queue.upgrade().unwrap().add(self.clone())
    }

    pub fn schedule_after(self:&Arc<Self>,us:u64){
        let entry = HRTEntry{ 
            deadline: SystemTime::now() + Duration::from_micros(us),
            workitem: self.clone()
        };
        HRT_QUEUE.add(entry);
    }

    /*
        schedule_until
        This method should be called at workqueue thread.
     */
    pub fn schedule_until(self:&Arc<Self>,us:u64){
        let entry = HRTEntry{ 
            deadline: self.last_call_time + Duration::from_micros(us),
            workitem: self.clone()
        };
        HRT_QUEUE.add(entry);
    }
}

pub struct WorkQueue{
    priority:i32,
    list:Mutex<VecDeque<Arc<WorkItem>>>,
    signal:Condvar,
    ready_exit:Mutex<bool>,
    thread_id:libc::pthread_t
}

extern "C" fn workqueue_thread_handler(ptr: *mut libc::c_void) -> *mut libc::c_void{
    unsafe{
        let wq = ptr as *mut WorkQueue;
        (*wq).run();
    }
    return null_mut();
}

impl WorkQueue{
    pub fn new(stack_size:u32,priority:i32,is_fifo_schedule:bool) -> Arc<WorkQueue>{
        let mut wq: WorkQueue<> = WorkQueue{
            priority,
            list:Mutex::new(VecDeque::new()),
            signal:Condvar::new(),
            ready_exit:Mutex::new(false),
            thread_id:0
        };
        let x = Arc::new_cyclic(|weak|{
            let _thread_id = create_phtread(stack_size, priority, workqueue_thread_handler,  weak.as_ptr() as *mut libc::c_void,is_fifo_schedule); 
            wq.thread_id = _thread_id;
            wq
        });

        x
    }

    pub fn run(&self){
 
        loop{
            {
                let exit= self.ready_exit.lock().unwrap();
                if *exit{
                    break;
                }
            }
            
            loop{
                let head = {
                    let mut x = self.list.lock().unwrap();
                    x.pop_front()
                };
                
                if let Some(item) = head {
                    let ptr = Arc::as_ptr(&item) as *mut WorkItem;
                    unsafe{
                        (*ptr).last_call_time = SystemTime::now();
                        // update the call time 
                        // the last_call_time MUST only be updated here, and be used in the workqueue thread
                        // if so, we can directly update it in the unsafe block rather than using a RwLock.
                    }

                    (item.func)(item.parent as *mut libc::c_void);
                }else{
                    break;
                }
            }

            let _ = self.signal.wait(self.list.lock().unwrap()); 
            // wait for other thread add item to queue          
        }
    }

    pub fn add(&self, item:Arc<WorkItem>){
        
        self.list.lock().unwrap().push_back(item);
        self.signal.notify_one();
    }

    pub fn exit(&self){
        *self.ready_exit.lock().unwrap() = true;
    }
}


#[cfg(test)]
pub mod tests{
    use super::*;

    pub struct GPS{
        pub item:Arc<WorkItem>,
        pub finish:bool
    }
    
    impl GPS {
        fn run(ptr:*mut libc::c_void){
            // actually we should not get a mut ptr and directly change its value here
            // as there may be other threads rely on the it
            // while it is just for test here, don't do more work to make self worried
            let gps = unsafe{
                &mut *(ptr as *mut Self)
            };
            gps.finish = true;
            println!("GPS is running!");
        }

        pub fn new(wq:&Arc<WorkQueue>) -> Arc<GPS> {
            let gps= Arc::new_cyclic(
                |gps_weak|{

                    let item = WorkItem::new(wq,gps_weak.as_ptr() as *mut GPS,GPS::run);
                    GPS{
                        item,
                        finish:false
                    }
                }
            );
            gps
        }
    }

    #[test]
    fn test_workqueue_basic(){
        let wq = WorkQueue::new(2048,10,false);

        let gps = GPS::new(&wq);
        gps.item.schedule();
        while gps.finish==false{};
        wq.exit();
    }

    #[test]
    fn test_workitem_schedule_after(){
        let wq = WorkQueue::new(2048,10,false);

        let gps = GPS::new(&wq);
        gps.item.schedule_after(3 * 1000 * 1000);
        std::thread::sleep(Duration::from_secs(1));
        assert_eq!(gps.finish,false);
        std::thread::sleep(Duration::from_secs(3)); 
        assert_eq!(gps.finish,true);
    }

    #[test]
    fn test_workitem_schedule_until(){
        let wq = WorkQueue::new(2048,10,false);

        let gps = GPS::new(&wq);
        // at present, the last_call_time of gps.item is SystemTime::now()

        std::thread::sleep(Duration::from_secs(1));
        gps.item.schedule_until(2*1000*1000);
        std::thread::sleep(Duration::from_millis(500)); 
        assert_eq!(gps.finish,false);
        std::thread::sleep(Duration::from_millis(700));  
        assert_eq!(gps.finish,true); 
    }
}
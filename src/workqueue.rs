use std::{sync::{Mutex, Condvar, Arc, Weak, LazyLock,RwLock}, collections::{VecDeque, HashMap}, ptr::null_mut, any::Any};
use crate::{pthread::create_phtread, msg::MSGSubscriber, hrt::{get_time_now, Timespec}};
use crate::hrt::{HRTEntry,HRT_QUEUE};


pub static WORKQUEUE_LIST:LazyLock<RwLock<HashMap<&str,Arc<WorkQueue>>>> = LazyLock::new(||{
    let m = HashMap::new();
    RwLock::new(m)
});

pub trait Callable {
    fn call(&mut self);
}

pub struct WorkItem{
    queue:Weak<WorkQueue>,
    func:fn(* mut libc::c_void),
    parent:* mut dyn Any,
    last_call_time:Timespec
}

unsafe impl Send for WorkItem{}
unsafe impl Sync for WorkItem{}
impl WorkItem{
    pub fn new<'a>(wq:&Arc<WorkQueue>,name:&'static str,parent:* mut dyn Any, func:fn(* mut libc::c_void)) -> Arc<WorkItem>{
        let x= Arc::new(
            WorkItem{
                queue:Arc::downgrade(wq),
                func,
                parent,
                last_call_time:get_time_now()
            }
        );

        wq.add_child_workitem(name,Arc::clone(&x));
        x
    }

    pub fn schedule(self:&Arc<Self>){  
        self.queue.upgrade().unwrap().add_to_worker_list(self.clone())
    }

    pub fn schedule_after(self:&Arc<Self>,us:i64){
        let entry = HRTEntry{ 
            deadline: get_time_now() + us * 1000,
            workitem: self.clone()
        };
        HRT_QUEUE.add(entry);
    }

    /*
        schedule_until
        This method should be called at workqueue thread.
     */
    pub fn schedule_until(self:&Arc<Self>,us:i64){
        let entry = HRTEntry{ 
            deadline: self.last_call_time + us * 1000,
            workitem: self.clone()
        };
        HRT_QUEUE.add(entry);
    }

    #[inline(always)]
    fn msg_subscriber_callback(ptr:*mut usize){
        unsafe{
            Arc::from_raw(ptr as *mut Self).schedule();
        }
    }

    pub fn bind_msg_subscriber(self:&Arc<Self>,sub:&Arc<MSGSubscriber>){
        sub.register_callback(Self::msg_subscriber_callback, Arc::into_raw(self.clone()) as *mut usize);
    }
}

pub struct WorkQueue{
    pub priority:i32,
    list:Mutex<VecDeque<Arc<WorkItem>>>,
    signal:Condvar,
    ready_exit:Mutex<bool>,
    thread_id:libc::pthread_t,
    child_items:RwLock<HashMap<&'static str,Arc<WorkItem>>>
}

extern "C" fn workqueue_thread_handler(ptr: *mut libc::c_void) -> *mut libc::c_void{
    unsafe{
        let wq = ptr as *mut WorkQueue;
        (*wq).run();
    }
    return null_mut();
}

impl WorkQueue{
    pub fn new(name:&'static str,stack_size:u32,priority:i32,is_fifo_schedule:bool) -> Arc<WorkQueue>{
        let mut wq: WorkQueue<> = WorkQueue{
            priority,
            list:Mutex::new(VecDeque::new()),
            signal:Condvar::new(),
            ready_exit:Mutex::new(false),
            thread_id:0,
            child_items:RwLock::new(HashMap::new())
        };
        let x = Arc::new_cyclic(|weak|{
            let _thread_id = create_phtread(stack_size, priority, workqueue_thread_handler,  weak.as_ptr() as *mut libc::c_void,is_fifo_schedule); 
            wq.thread_id = _thread_id;
            wq
        });

        WORKQUEUE_LIST.write().unwrap().insert(name, Arc::clone(&x));
        x
    }

    pub fn find(name:&'static str)->Arc<WorkQueue>{
        let list = WORKQUEUE_LIST.read().unwrap();
        let x = list.get(name).unwrap();
        x.clone()
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
                        (*ptr).last_call_time = get_time_now();
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

    pub fn add_to_worker_list(&self, item:Arc<WorkItem>){
        
        self.list.lock().unwrap().push_back(item);
        self.signal.notify_one();
    }

    pub fn add_child_workitem(&self,name:&'static str, item:Arc<WorkItem>){
        let mut list = self.child_items.write().unwrap();
        list.insert(name, item);
    }

    pub fn exit(&self){
        *self.ready_exit.lock().unwrap() = true;
    }
}


#[cfg(test)]
pub mod tests{
    use super::*;
    use crate::lock_step::lock_step_init_test_thread;
    use core::time::Duration;

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

                    let item = WorkItem::new(wq,"gps",gps_weak.as_ptr() as *mut GPS,GPS::run);
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
        let wq = WorkQueue::new("basic",2048,10,false);

        let gps = GPS::new(&wq);
        gps.item.schedule();
        while gps.finish==false{};
        wq.exit();
    }

    #[test]
    fn test_workitem_schedule_after(){
        let wq = WorkQueue::new("schedule_after",2048,10,false);

        let gps = GPS::new(&wq);
        gps.item.schedule_after(3 * 1000 * 1000);
        std::thread::sleep(Duration::from_secs(1));
        assert_eq!(gps.finish,false);
        std::thread::sleep(Duration::from_secs(3)); 
        assert_eq!(gps.finish,true);
    }

    #[test]
    fn test_workitem_schedule_until(){
        #[cfg(feature="lock_step_enabled")]
        lock_step_init_test_thread();
        let wq = WorkQueue::new("schedule_until",2048,10,false);

        let gps = GPS::new(&wq);
        // at present, the last_call_time of gps.item is SystemTime::now()

        std::thread::sleep(Duration::from_secs(1));
        gps.item.schedule_until(2*1000*1000);
        std::thread::sleep(Duration::from_millis(500)); 
        assert_eq!(gps.finish,false);
        std::thread::sleep(Duration::from_millis(700));  
        assert_eq!(gps.finish,true); 
    }

    use crate::msg::{tests::*, MSG_LIST, MSGPublisher};
    #[test]
    fn test_workitem_bind_message(){
        let wq = WorkQueue::new("msg_bind",2048,10,false);

        let gps = GPS::new(&wq); 

        add_message_entry::<GyroMSG>("gyro");

        let publisher = MSGPublisher::new("gyro");
        let subscriber = MSGSubscriber::new("gyro").unwrap();

        gps.item.bind_msg_subscriber(&subscriber);

        let test_data = get_test_gyromsg();
        publisher.publish(&test_data);

        std::thread::sleep(Duration::from_secs(1));
        assert_eq!(gps.finish,true);
    }
}
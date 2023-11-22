use std::{sync::{Mutex, Condvar, Arc, Weak}, collections::VecDeque, thread::{spawn}, borrow::{Borrow, BorrowMut}, ptr::null_mut};
use crate::pthread::create_phtread;


pub trait Callable {
    fn call(&self);
}

pub struct WorkItem{
    queue:Weak<WorkQueue>,
    parent:Weak<dyn Callable + Sync + Send>,
}

impl WorkItem{
    pub fn schedule(self:&Arc<Self>){  
        self.queue.upgrade().unwrap().add(self.parent.upgrade().unwrap())
    }

    pub fn new(queue:&Arc<WorkQueue>,parent:&Arc<dyn Callable + Sync + Send>)->WorkItem{
        WorkItem {
            queue:Arc::downgrade(queue), 
            parent:Arc::downgrade(parent as &Arc<dyn Callable + Send + Sync>)
        }
    }
}

pub struct WorkQueue{
    priority:i32,
    list:Mutex<VecDeque<Arc<dyn Callable + Sync + Send>>>,
    signal:Condvar,
    ready_exit:Mutex<bool>
}

extern "C" fn workqueue_thread_handler(ptr: *mut libc::c_void) -> *mut libc::c_void{
    unsafe{
        let wq = ptr as *mut WorkQueue;
        (*wq).run();
    }
    return null_mut();
}

impl WorkQueue{
    pub fn new(stack_size:u32,priority:i32) -> Arc<WorkQueue>{
        let wq: WorkQueue<> = WorkQueue{
            priority,
            list:Mutex::new(VecDeque::new()),
            signal:Condvar::new(),
            ready_exit:Mutex::new(false)
        };
        let x = Arc::new(wq);

        let x2 = Arc::clone(&x);
        create_phtread(stack_size, priority, workqueue_thread_handler, Arc::as_ptr(&x).cast_mut() as *mut libc::c_void,true);
        x2
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
                    item.call();
                }else{
                    break;
                }
            }

            let _ = self.signal.wait(self.list.lock().unwrap()); 
            // wait for other thread add item to queue          
        }
    }

    pub fn add(&self, item:Arc<dyn Callable + Send + Sync>){
        
        self.list.lock().unwrap().push_back(item);
        self.signal.notify_one();
    }

    pub fn exit(&self){
        *self.ready_exit.lock().unwrap() = true;
    }
}


#[cfg(test)]
mod tests{
    use super::*;
    #[test]
    fn test_workqueue_basic(){

    }
}
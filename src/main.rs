use std::{sync::{Mutex, Condvar, Arc, Weak}, collections::VecDeque, thread::{spawn}, borrow::BorrowMut};


struct GPS{

}

impl Callable for GPS{
    fn call(&self) {
        println!("GPS Thread!");
    }
}


struct WorkItem{
    queue:Weak<WorkQueue>,
    parent:Weak<dyn Callable + Sync + Send>
}

trait Callable {
    fn call(&self);
}

impl WorkItem{
    fn schedule(self:&Arc<Self>){  
        self.queue.upgrade().unwrap().add(Arc::clone(self))
    }
}

struct WorkQueue{
    priority:u8,
    list:Mutex<VecDeque<Arc<WorkItem>>>,
    signal:Condvar,
    ready_exit:Mutex<bool>
}

impl WorkQueue{
    fn new(priority:u8) -> Arc<WorkQueue>{
        let wq: WorkQueue<> = WorkQueue{
            priority,
            list:Mutex::new(VecDeque::new()),
            signal:Condvar::new(),
            ready_exit:Mutex::new(false)
        };
        let x = Arc::new(wq);

        let x2 = Arc::clone(&x);
        spawn(move || x.run());
        x2
    }

    fn run(&self){
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
                    item.parent.upgrade().unwrap().call();
                }else{
                    break;
                }
            }

            let _ = self.signal.wait(self.list.lock().unwrap()); 
            // wait for other thread add item to queue          
        }
    }

    fn add(&self, item:Arc<WorkItem>){
        
        self.list.lock().unwrap().push_back(item);
        self.signal.notify_one();
    }

    fn exit(&self){
        *self.ready_exit.lock().unwrap() = true;
    }
}

fn main() {
    let wq = WorkQueue::new(0);
    let gps  = Arc::new(GPS{});
    let mut item = WorkItem{queue:Weak::new(),parent:Arc::downgrade(&(gps.clone() as Arc<dyn Callable + Send + Sync>))};

    *item.queue.borrow_mut() = Arc::downgrade(&wq);
    let x = Arc::new(item);
    x.schedule();
    println!("Hello, world!");
    loop{};
}

use std::{sync::{Mutex, Condvar, Arc, Weak}, collections::VecDeque, thread::{spawn}, borrow::BorrowMut, time::{Duration, SystemTime}, mem::MaybeUninit};
mod pthread;
mod workqueue;

use workqueue::*;

struct HRTEntry{
    deadline:SystemTime,
    workitem:Arc<WorkItem>
}

struct HRTQueue{
    list:Mutex<VecDeque<HRTEntry>>,
}

impl HRTQueue{
    fn run(&self){
        let mut sleep_time = Duration::from_millis(1);   // default to sleep 1ms
        loop{
            let mut unlock_list = self.list.lock().unwrap();
            if let Some(x) = unlock_list.front(){
                let now = SystemTime::now();
                if now >= x.deadline{
                    x.workitem.schedule();

                    unlock_list.pop_front();
                }else{
                    sleep_time = x.deadline.duration_since(now).unwrap();
                }
            }

            std::thread::sleep(sleep_time);
        }
    }

    fn add(&self,entry:HRTEntry){
        let mut unlock_list = self.list.lock().unwrap();
        let now = SystemTime::now();

        if unlock_list.is_empty() || unlock_list.front().unwrap().deadline > now{
            unlock_list.push_front(entry);
        }else{
            let may_be_index = unlock_list.iter_mut().position(|x| x.deadline > now);
            match may_be_index{
                Some(index) => unlock_list.insert(index, entry),
                None => unlock_list.push_back(entry)
            };
        }


    }
}

fn main() {
    println!("Hello, world!");
}

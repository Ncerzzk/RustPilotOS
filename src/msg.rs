
use std::{sync::{LazyLock, RwLock, Arc, Weak, Mutex}, collections::HashMap, mem::{self, MaybeUninit}};

struct MSGPublisher{
    parent:Weak<MSGEntry>
}

impl MSGPublisher{
    fn new(name:&'static str)->&'static Self{
        &(MSGEntry::find(name).unwrap().publisher)
    }

    fn publish<T:MessageMetaData>(&self,data:&T){
        let entry = self.parent.upgrade().unwrap();
        let mut msg = entry.msg.write().unwrap();
        unsafe{
            std::ptr::copy_nonoverlapping(data as *const T, msg.data.as_mut() as *mut(dyn MessageMetaData+'static) as *mut T, 1);
        }
        msg.index  += 1;
    }
}

struct MSGSubscriber{
    parent:Weak<MSGEntry>,
    last_index:u32
}

impl MSGSubscriber{
    fn new(name:&'static str)-> Arc<Self>{
        let entry = MSGEntry::find(name).unwrap();
        let sub = Arc::new(MSGSubscriber{
            parent:Arc::downgrade(entry),
            last_index:0
        });
        entry.subscribers.write().unwrap().push(sub.clone());
        sub
    }

    fn check_update(self:&Arc::<Self>)->bool{
       self.last_index != self.parent.upgrade().unwrap().msg.read().unwrap().index 
    }

    fn get_latest<T:MessageMetaData>(self:&Arc::<Self>)->T{
        let entry = &self.parent.upgrade().unwrap();
        let msg = &entry.msg.read().unwrap();
        let mut ret = MaybeUninit::<T>::zeroed();
        let data = &msg.data;
        unsafe{
            let self_ptr = self.as_ref() as *const Self as *mut Self;
            (*self_ptr).last_index = msg.index;
            std::ptr::copy_nonoverlapping(data.as_ref() as *const dyn MessageMetaData as *const T, ret.as_mut_ptr(), 1);

            ret.assume_init()
        }
    }
}

trait MessageMetaData{}

struct Message{
    data:Box<dyn MessageMetaData>,
    index:u32
}
struct MSGEntry{
    name:&'static str,
    publisher:MSGPublisher,
    subscribers:RwLock<Vec<Arc<MSGSubscriber>>>,
    msg:RwLock<Message>
}

unsafe impl Send for MSGEntry{}
unsafe impl Sync for MSGEntry{}

impl MSGEntry{
    fn new<T:MessageMetaData + 'static>(name:&'static str) ->Arc<Self>{
        Arc::new_cyclic(|weak|{
            let message = Message{
                data:unsafe{Box::<T>::new_zeroed().assume_init()},
                index:0
            };

            MSGEntry{
                name,
                publisher:MSGPublisher{parent:weak.clone()},
                subscribers:RwLock::new(Vec::new()),
                msg:RwLock::new(message)
            }
        })
    }

    #[inline]
    fn find(name:&str)->Option<&Arc<Self>>{
        (*MSG_LIST).get(name)
    }
}



static MSG_LIST:LazyLock<HashMap<&str, Arc<MSGEntry>>> = LazyLock::new(|| {
    let mut map = HashMap::new();
    map
});


#[cfg(test)]
mod tests{
    use super::*;

    #[derive(PartialEq,Debug)]
    struct GyroMSG{
        x:f32,
        y:f32,
        z:f32
    }

    impl MessageMetaData for GyroMSG {}

    fn get_test_gyromsg()->GyroMSG{
        GyroMSG{
            x:50.0,
            y:50.0,
            z:10.0
        }
    }

    fn add_message_entry<T:MessageMetaData + 'static>(name:&'static str){
        let list = &(*MSG_LIST) as *const HashMap<&str,Arc<MSGEntry>> as *mut HashMap<&str,Arc<MSGEntry>> ;
        unsafe{
            (*list).insert(name, MSGEntry::new::<T>(name));
        }
    }

    #[test]
    fn test_message_list_add(){
        assert_eq!(MSG_LIST.len(),0);
        add_message_entry::<GyroMSG>("gyro");
        assert_eq!(MSG_LIST.len(),1);
    }

    #[test]
    fn test_msg_publish_and_subscribe(){
        add_message_entry::<GyroMSG>("gyro");

        let imu = MSGPublisher::new("gyro");
        let test_data = get_test_gyromsg();
        imu.publish(&test_data);

        let gyro_msg_entry = MSGEntry::find("gyro").unwrap();
        let msg = gyro_msg_entry.msg.read().unwrap();
        assert_eq!(msg.index,1);
        unsafe{
            let a = msg.data.as_ref() as *const dyn MessageMetaData as *const GyroMSG;
            assert_eq!(*a,test_data);
        }

        let suber = MSGSubscriber::new("gyro");
        assert_eq!(suber.check_update(),true);
        assert_eq!(suber.get_latest::<GyroMSG>(),test_data);
    }

}






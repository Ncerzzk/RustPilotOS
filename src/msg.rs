
use std::{sync::{LazyLock, RwLock, Arc, Weak}, collections::HashMap, mem::{MaybeUninit}};

pub struct MSGPublisher{
    parent:Weak<MSGEntry>
}

impl MSGPublisher{
    pub fn new(name:&'static str)->&'static Self{
        &(MSGEntry::find(name).unwrap().publisher)
    }

    pub fn publish<T:MessageMetaData>(&self,data:&T){
        let entry = self.parent.upgrade().unwrap();
        {
            let mut msg = entry.msg.write().unwrap();
            unsafe{
                std::ptr::copy_nonoverlapping(data as *const T, msg.data.as_mut() as *mut(dyn MessageMetaData+'static) as *mut T, 1);
            }
            msg.index  += 1;
        }

        let mut sub_list = entry.subscribers.write().unwrap();
        let mut remove_list = Vec::new();
        for (index,i) in sub_list.iter().enumerate(){
            if let Some(subscriber) = i.upgrade(){
                if let Some(func) = subscriber.callback{
                    func(subscriber.callback_arg.unwrap())
                }
            }else {
                // weak upgrade failed
                remove_list.push(index);
                
            }
        }

        if remove_list.len() > 0 {
            for i in remove_list.iter().rev(){
                sub_list.swap_remove(*i);
            }
        }
    }
}

pub struct MSGSubscriber{
    parent:Weak<MSGEntry>,
    last_index:u32,
    callback:Option<fn (*mut usize)>,
    callback_arg:Option<*mut usize>
}

impl MSGSubscriber{
    pub fn new(name:&'static str)-> Arc<Self>{
        let entry = MSGEntry::find(name).unwrap();
        let mut subscribers = entry.subscribers.write().unwrap();
        
        let sub = Arc::new(MSGSubscriber{
            parent:Arc::downgrade(entry),
            last_index:0,
            callback:Option::None,
            callback_arg:Option::None
        });
        subscribers.push(Arc::downgrade(&sub));
        sub
    }

    pub fn check_update(self:&Arc::<Self>)->bool{
       self.last_index != self.parent.upgrade().unwrap().msg.read().unwrap().index 
    }

    pub fn get_latest<T:MessageMetaData>(self:&Arc::<Self>)->T{
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

    pub fn register_callback(self:&Arc::<Self>,callback:fn (*mut usize),arg:*mut usize){
        let ptr = self.as_ref() as *const Self as *mut Self;
        unsafe{ 
            (*ptr).callback = Option::Some(callback);
            (*ptr).callback_arg = Option::Some(arg);
        }
    }
}

pub trait MessageMetaData{}

pub struct Message{
    data:Box<dyn MessageMetaData>,
    index:u32
}
pub struct MSGEntry{
    pub name:&'static str,
    publisher:MSGPublisher,
    subscribers:RwLock<Vec<Weak<MSGSubscriber>>>,
    msg:RwLock<Message>
}

unsafe impl Send for MSGEntry{}
unsafe impl Sync for MSGEntry{}

impl MSGEntry{
    pub fn new<T:MessageMetaData + 'static>(name:&'static str) ->Arc<Self>{
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
    pub fn find(name:&str)->Option<&Arc<Self>>{
        (*MSG_LIST).get(name)
    }
}



pub static MSG_LIST:LazyLock<HashMap<&str, Arc<MSGEntry>>> = LazyLock::new(|| {
    let map = HashMap::new();
    map
});


#[cfg(test)]
pub mod tests{
    use super::*;

    #[derive(PartialEq,Debug)]
    pub struct GyroMSG{
        x:f32,
        y:f32,
        z:f32
    }

    impl MessageMetaData for GyroMSG {}

    pub fn get_test_gyromsg()->GyroMSG{
        GyroMSG{
            x:50.0,
            y:50.0,
            z:10.0
        }
    }

    pub fn add_message_entry<T:MessageMetaData + 'static>(name:&'static str){
        let list = &(*MSG_LIST) as *const HashMap<&str,Arc<MSGEntry>> as *mut HashMap<&str,Arc<MSGEntry>>;

        unsafe{
            if !(*list).contains_key(name){
                (*list).insert(name, MSGEntry::new::<T>(name));
            }
        }
    }

    #[test]
    fn test_message_list_add(){
        let len_old = MSG_LIST.len();
        add_message_entry::<GyroMSG>("just testing");
        assert_eq!(MSG_LIST.len(),len_old + 1);
    }


    fn test_callback(ptr:*mut usize){
        unsafe{
            (*ptr) +=1
        }
        println!("sub callback!");
    }
    #[test]
    fn test_msg_publish_and_subscribe(){
        add_message_entry::<GyroMSG>("gyro");

        let suber = MSGSubscriber::new("gyro");
        let mut num:usize = 0;
        suber.register_callback(test_callback, &mut num as *mut usize);

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

        assert_eq!(suber.check_update(),true);
        assert_eq!(suber.get_latest::<GyroMSG>(),test_data);
        assert_eq!(num,1);
    }
    #[test]
    fn test_subscriber_drop(){
        add_message_entry::<GyroMSG>("gyro_tttt");

        let suber1 = MSGSubscriber::new("gyro_tttt"); 
        {
            let suber2 = MSGSubscriber::new("gyro_tttt");
            assert_eq!(MSGEntry::find("gyro_tttt").unwrap().subscribers.read().unwrap().len(),2);
        }

        let imu = MSGPublisher::new("gyro_tttt");
        let test_data = get_test_gyromsg();
        imu.publish(&test_data);

        assert_eq!(MSGEntry::find("gyro_tttt").unwrap().subscribers.read().unwrap().len(),1);        
    }

}






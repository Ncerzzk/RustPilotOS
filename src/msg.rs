
use std::{sync::{LazyLock, RwLock, Arc, Weak, Mutex}, collections::HashMap};

struct MSGPublisher{
    msg:Weak<Mutex<MSG>>
}

impl MSGPublisher{
    fn new(name:&'static str)->&'static Self{
        &(MSG::find(name).unwrap().publisher)
    }

    fn publish(&self,data:Box<dyn MessageMetaData>){
        let msg = self.msg.upgrade().unwrap();
        let mut msg = msg.lock().unwrap(); 
        msg.data = data;
        msg.index  += 1;
    }
}

struct MSGSubscriber{

}

trait MessageMetaData {}

struct MSG{
    name:&'static str,
    publisher:MSGPublisher,
    subscribers:Vec<Arc<MSGSubscriber>>,
    data:Box<dyn MessageMetaData>,
    index:u32
}

unsafe impl Send for MSG{}
unsafe impl Sync for MSG{}

impl MSG{
    fn new(name:&'static str,data:Box<dyn MessageMetaData>) ->Arc<Mutex<Self>>{
        Arc::new_cyclic(|weak|{
            Mutex::new(MSG{
                name,
                publisher:MSGPublisher{msg:weak.clone()},
                subscribers:Vec::new(),
                data,
                index:0
            })
        })
    }

    #[inline]
    fn find(name:&str)->Option<&Self>{
        (*MSG_LIST).get(name)
    }
}

static MSG_LIST:LazyLock<HashMap<&str, MSG>> = LazyLock::new(|| {
    HashMap::new()
});

struct GyroMSG{
    x:f32,
    y:f32,
    z:f32
}

impl MessageMetaData for GyroMSG {}




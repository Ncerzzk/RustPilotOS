use std::{sync::{LazyLock, RwLock}, collections::HashMap, any::Any};
use crate::channel::{Receiver,Sender,Channel};

pub struct Message<T>{
    pub rx:Receiver<T>,
    pub tx:Sender<T>
}

pub struct MessageList{
    data:HashMap<&'static str,Box<dyn Any>>
}


impl MessageList{
    pub fn get_message<T:'static>(&self,name:&str)->Option<&Message<T>>{
        let a = self.data.get(name);
        if let Some(a) = a{
            let a = a.downcast_ref::<Message<T>>();
            a
        }else{
            None
        }
    }
}


pub fn add_message<T:Clone + 'static>(name:&'static str){
    let (tx,rx) = Channel::<T>::new();
    let msg= Message{
        rx,
        tx
    };
    MESSAGE_LIST.write().unwrap().data.insert(name, Box::new(msg));
}

pub fn get_new_tx_of_message<T:'static>(name:&str)->Option<Sender<T>>{
    let list= &MESSAGE_LIST.read().unwrap();
    let msg = list.get_message(name);
    if let Some(msg) = msg{
        Some(msg.tx.clone())
    }else{
        None
    }
}

pub fn get_new_rx_of_message<T:'static>(name:&str)->Option<Receiver<T>>{
    let list= &MESSAGE_LIST.read().unwrap();
    let msg = list.get_message(name);
    if let Some(msg) = msg{
        Some(msg.rx.clone())
    }else{
        None
    }    
}


unsafe impl Send for MessageList{}
unsafe impl Sync for MessageList{}

static MESSAGE_LIST:LazyLock<RwLock<MessageList>> = LazyLock::new(||{
    RwLock::new(MessageList { data:HashMap::new() })
});

#[cfg(test)]
mod tests{
    use super::*;

    #[derive(Debug,Clone,Copy)]
    struct GyroData{
        data:[i32;3]
    }

    #[ctor::ctor]
    fn ttt(){
        add_message::<GyroData>("test_gyro");
    }

    #[test]
    fn test_basic_message(){
        let mut rx = get_new_rx_of_message::<GyroData>("test_gyro").unwrap();
        let tx = get_new_tx_of_message::<GyroData>("test_gyro").unwrap();

        std::thread::spawn(move ||{
            tx.send(GyroData{ data: [1,2,3] });
        });

        let recv_data = rx.read().data;

        assert_eq!(recv_data[0],1);
        assert_eq!(recv_data[1],2);

        println!("data:{:?}",recv_data);

    }
}
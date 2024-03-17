use std::{
    collections::HashMap,
    mem::MaybeUninit,
    ptr::null_mut,
    sync::{
        atomic::{AtomicBool, AtomicU32, Ordering},
        Condvar, Mutex,
    },
};

pub struct Sender<T> {
    parent: *mut Channel<T>,
}

impl<T> Default for Sender<T> {
    fn default() -> Self {
        Self { parent: null_mut() }
    }
}

pub struct Receiver<T> {
    parent: *mut Channel<T>,
    last_cnt: u32,
}

impl<T> Default for Receiver<T> {
    fn default() -> Self {
        Self {
            parent: null_mut(),
            last_cnt: 0,
        }
    }
}

unsafe impl<T> Send for Sender<T> {}
unsafe impl<T> Sync for Sender<T> {}
unsafe impl<T> Send for Receiver<T> {}
unsafe impl<T> Sync for Receiver<T> {}

impl<T> Receiver<T>
where
    T: Clone,
{
    pub fn read(&mut self) -> T {
        let channel = unsafe { &mut *self.parent };
        if self.last_cnt == channel.cnt {
            // if no new msg, then wait for update
            channel.wait_for_update();
        }
        let (cnt, data) = channel.read();
        self.last_cnt = cnt;
        data
    }

    pub fn try_read(&mut self) -> Option<T> {
        let channel = unsafe { &mut *self.parent };
        let (cnt, data) = channel.read();
        let updated = self.last_cnt != cnt;

        if updated {
            Some(data)
        } else {
            None
        }
    }

    pub fn register_callback<F>(&self, name: &str, callback: F)
    where
        F: FnMut(&T) + 'static,
    {
        let channel = unsafe { &mut *self.parent };
        channel.register_callback(name, callback);
    }

    pub fn unregister_callback(&self, name: &str) {
        let channel = unsafe { &mut *self.parent };
        channel.unregister_callback(name);
    }
}

impl<T> Clone for Receiver<T> {
    fn clone(&self) -> Self {
        unsafe {
            (&mut *(self.parent))
                .receiver_cnt
                .fetch_add(1, Ordering::AcqRel)
        };
        Self {
            parent: self.parent.clone(),
            last_cnt: self.last_cnt.clone(),
        }
    }
}

impl<T> Drop for Receiver<T> {
    fn drop(&mut self) {
        if self.parent == null_mut() {
            return;
        }
        let channel = unsafe { &mut *(self.parent) };
        if channel.receiver_cnt.fetch_sub(1, Ordering::AcqRel) == 1 {
            if channel.destoryed.swap(true, Ordering::AcqRel) == true {
                drop(unsafe { Box::from_raw(self.parent) });
            }
        }
    }
}

impl<T> Sender<T>
where
    T: Clone,
{
    pub fn send(&self, data: T) {
        let channel = unsafe { &mut *(self.parent) };
        channel.write(data);
    }
}

impl<T> Clone for Sender<T> {
    fn clone(&self) -> Self {
        unsafe {
            (&mut *(self.parent))
                .sender_cnt
                .fetch_add(1, Ordering::AcqRel)
        };
        Self {
            parent: self.parent.clone(),
        }
    }
}

impl<T> Drop for Sender<T> {
    fn drop(&mut self) {
        if self.parent == null_mut() {
            return;
        }
        let channel = unsafe { &mut *(self.parent) };
        if channel.sender_cnt.fetch_sub(1, Ordering::AcqRel) == 1 {
            if channel.destoryed.swap(true, Ordering::AcqRel) == true {
                drop(unsafe { Box::from_raw(self.parent) });
            }
        }
    }
}

pub struct Channel<T> {
    data: MaybeUninit<T>,
    callbacks: Mutex<HashMap<String, Box<dyn FnMut(&T)>>>,
    cnt: u32,
    lock: Mutex<bool>, // this lock protect data,cnt
    condvar: Condvar,
    sender_cnt: AtomicU32,
    receiver_cnt: AtomicU32,
    destoryed: AtomicBool,
}

unsafe impl<T> Send for Channel<T> {}
unsafe impl<T> Sync for Channel<T> {}

impl<T> Channel<T>
where
    T: Sized + Clone,
{
    pub fn new() -> (Sender<T>, Receiver<T>) {
        let channel = Box::new(Channel {
            data: MaybeUninit::zeroed(),
            callbacks: Mutex::new(HashMap::new()),
            cnt: 0,
            lock: Mutex::new(false),
            condvar: Condvar::new(),
            sender_cnt: AtomicU32::new(1),
            receiver_cnt: AtomicU32::new(1),
            destoryed: AtomicBool::new(false),
        });

        let channel_ptr = Box::into_raw(channel);
        let tx = Sender {
            parent: channel_ptr,
        };
        let rx = Receiver {
            parent: channel_ptr,
            last_cnt: 0,
        };

        (tx, rx)
    }

    fn register_callback<F>(&mut self, name: &str, callback: F)
    where
        F: FnMut(&T) + 'static,
    {
        self.callbacks
            .lock()
            .unwrap()
            .insert(name.to_string(), Box::new(callback));
    }

    fn unregister_callback(&mut self, name: &str) {
        self.callbacks.lock().unwrap().remove(&name.to_string());
    }

    fn write(&mut self, msg: T) {
        for (_, callback) in self.callbacks.lock().unwrap().iter_mut() {
            callback(&msg);
        }

        {
            let _a = self.lock.lock().unwrap();
            self.data.write(msg);
            self.cnt += 1;
        }
        self.condvar.notify_all();
    }

    fn read(&self) -> (u32, T)
    where
        T: Clone,
    {
        let _a = self.lock.lock().unwrap();
        (self.cnt, unsafe { self.data.assume_init_ref().clone() })
    }

    fn wait_for_update(&self) {
        drop(self.condvar.wait(self.lock.lock().unwrap()).unwrap());
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    #[derive(Debug, Default, Clone, Copy)]
    struct TestStruct {
        x: u32,
        y: u32,
        z: u32,
    }

    #[test]
    fn test_basic_rxtx() {
        let (tx, mut rx) = Channel::<TestStruct>::new();

        tx.send(TestStruct::default());
        rx.read();
        assert_eq!(rx.last_cnt, 1);

        let try_result = rx.try_read();
        assert!(try_result.is_none());
    }

    #[test]
    fn test_channel_drop() {
        let (tx, mut rx) = Channel::<TestStruct>::new();
        {
            let rx2 = rx.clone();
            assert_eq!(
                unsafe { (&*(rx.parent)).receiver_cnt.load(Ordering::Relaxed) },
                2
            );
        }
        assert_eq!(
            unsafe { (&*(rx.parent)).receiver_cnt.load(Ordering::Relaxed) },
            1
        );

        drop(tx);
        assert_eq!(
            unsafe { (&*(rx.parent)).sender_cnt.load(Ordering::Relaxed) },
            0
        );
        assert_eq!(
            unsafe { (&*(rx.parent)).destoryed.load(Ordering::Relaxed) },
            true
        );
    }

    #[test]
    fn test_block_read() {
        let (tx, mut rx) = Channel::<TestStruct>::new();
        let start_time = std::time::SystemTime::now();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_secs(5));
            tx.send(TestStruct::default());
        });
        rx.read();

        let escape = start_time.elapsed().unwrap();

        assert!(escape.as_secs() >= 5);
    }

    #[test]
    fn test_channel_callback() {
        fn test_func(msg: &TestStruct) {
            std::thread::sleep(std::time::Duration::from_secs(5));
        }
        let (tx, mut rx) = Channel::<TestStruct>::new();
        rx.register_callback("test_cb", test_func);

        let start_time = std::time::SystemTime::now();
        tx.send(TestStruct::default());
        assert!(start_time.elapsed().unwrap().as_secs() >= 5);

        let start_time = std::time::SystemTime::now();
        rx.unregister_callback("test_cb");
        tx.send(TestStruct::default());
        assert!(start_time.elapsed().unwrap().as_secs() <= 1);
    }
}

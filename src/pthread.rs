use std::mem::MaybeUninit;



pub fn create_phtread(stack_size:u32, priority:i32, f: extern "C" fn(*mut libc::c_void) -> *mut libc::c_void,value:*mut libc::c_void, is_fifo_schedule:bool) -> u64 {
    unsafe{
        let mut attr = MaybeUninit::<libc::pthread_attr_t>::uninit();
        let attr_ptr = attr.as_mut_ptr();
        libc::pthread_attr_init(attr_ptr);
        libc::pthread_attr_setstacksize(attr_ptr, stack_size as usize);
        libc::pthread_attr_setinheritsched(attr_ptr, libc::PTHREAD_EXPLICIT_SCHED);
        {
            let x:i32;
            if is_fifo_schedule{
                x = libc::pthread_attr_setschedpolicy(attr_ptr,libc::SCHED_FIFO);
            }else{
                x = libc::pthread_attr_setschedpolicy(attr_ptr,libc::SCHED_OTHER); 
            }
        }

        
        let param = libc::sched_param{sched_priority:priority};
        libc::pthread_attr_setschedparam(attr_ptr,&param);

        let mut pthread = MaybeUninit::<libc::pthread_t>::uninit();
        libc::pthread_create(pthread.as_mut_ptr(), attr_ptr, f, value);
        pthread.assume_init()
    }
    
}

#[cfg(test)]
mod tests{
    use super::*;
    extern "C" fn testfunc(_:*mut libc::c_void) -> *mut libc::c_void{
        println!("hello,this is a test func!");
        return std::ptr::null_mut();
    }
    #[test]
    fn test_create_pthread() {
        create_phtread(2048, 1, testfunc, 0 as *mut libc::c_void,false);
    }
}

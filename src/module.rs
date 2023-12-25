use std::{sync::{LazyLock, RwLock, Arc}, collections::HashMap};



unsafe impl Send for Module{}
unsafe impl Sync for Module{}

pub type SubCallbackBox = Box<dyn Fn(u32,*const &str)>;
pub struct Module{
    name:&'static str,
    init_func:Box<dyn Fn(u32,*const &str) + 'static>
}

static MODULE_LIST:LazyLock<RwLock<HashMap<&str, Arc<Module>>>> = LazyLock::new(|| {
    let map = HashMap::new();
    RwLock::new(map)
});

impl Module{
    pub fn register<T>(name:&'static str,func:T) ->() where T:Fn(u32,*const &str) + 'static{
        let m = Module{
            name,
            init_func:Box::new(func)
        };
        MODULE_LIST.write().unwrap().insert(name, Arc::new(m));
    }

    pub fn get_module(name:&str)->Arc<Module>{
        MODULE_LIST.read().unwrap().get(name).unwrap().clone()
    }

    pub fn execute(self:&Arc<Self>, argc:u32,argv:*const &str){
        let p = &(self.init_func);
        p(argc,argv);        
    }
}


#[cfg(test)]
mod tests{
    use super::*;
    use ctor::ctor;

    fn test_func(argc:u32,argv:*const &str){
        println!("hello,this is test function!");
        assert_eq!(argc,1);
    }

    #[ctor]
    fn register() {
        Module::register("test", test_func);
    }

    #[test]
    fn test_module_register(){
        assert_eq!(MODULE_LIST.read().unwrap().len(),1);
        Module::get_module("test").execute(1, std::ptr::null());
    }

}

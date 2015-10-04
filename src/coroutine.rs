use std::borrow::Borrow;
use std::boxed::FnBox;
use std::mem;

use libc;
use context::{
    Context,
    Stack,
};

pub struct Coroutine {
    pub context: Context,
}

extern "C" fn context_func(parent_context_ptr: usize, boxed_context_body: *mut libc::c_void) -> ! {
    unsafe {
        let context_body: Box<Box<FnBox()>> = Box::from_raw(boxed_context_body as *mut Box<FnBox()>);
        context_body()
    };

    let parent_context: &mut Context = unsafe { mem::transmute(parent_context_ptr) };
    Context::load(parent_context);

    unreachable!("Coros internal error, execution should never reach here");
}

impl Coroutine {
    pub fn new(coroutine_body: Box<FnOnce() + Send + 'static>, parent_context: &Context, stack: &mut Stack) -> Coroutine
    {
        let parent_context_ptr = {
            &*parent_context.borrow() as *const Context
        };
        let context = Context::new(
            context_func,
            parent_context_ptr as usize,
            Box::into_raw(Box::new(coroutine_body)) as *mut libc::c_void,
            stack,
        );

        Coroutine {
            context: context,
        }
    }
}

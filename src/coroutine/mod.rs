use std::borrow::Borrow;
use std::boxed::FnBox;
use std::mem;

use context::{
    Context,
    Stack,
};
use mio::EventLoop;

pub mod io_handle;
pub mod join_handle;
pub mod channel;

use IoHandle;
use Result;
use scheduler::{
    BlockedCoroutineSlab,
    Scheduler,
};

#[derive(Debug)]
pub enum CoroutineState {
    New,
    Running,
    Blocked,
}

extern "C" fn context_init(coroutine_ptr: usize, scheduler_context_ptr: usize) -> ! {
    let coroutine: &mut Coroutine = unsafe { mem::transmute(coroutine_ptr) };
    let function = coroutine
        .function
        .take()
        .expect("Coros internal error: cannot run coroutine without function");
    let scheduler_context: &Context = unsafe { mem::transmute(scheduler_context_ptr) };
    let coroutine_blocking_handle = IoHandle {
        coroutine: coroutine,
        scheduler_context: scheduler_context,
    };

    function.call_box((coroutine_blocking_handle,));

    Context::load(&scheduler_context);

    unreachable!("Coros internal error: execution should never reach here");
}

pub type EventLoopRegistrationCallback = Box<FnBox(Coroutine, &mut EventLoop<Scheduler>, &mut BlockedCoroutineSlab) -> Result<()>>;

pub struct Coroutine {
    pub context: Context,
    function: Option<Box<FnBox(IoHandle) + Send + 'static>>,
    pub event_loop_registration: Option<EventLoopRegistrationCallback>,
    pub state: CoroutineState,
}

impl Clone for Coroutine {
    fn clone(&self) -> Coroutine {
        unreachable!("Apparently needed for deque, but should never be used");
    }
}

unsafe impl Send for Coroutine {}

impl Coroutine {
    pub fn new(
        function: Box<FnBox(IoHandle) + Send + 'static>,
        stack: Stack,
    ) -> Coroutine
    {
        let context = Context::new(
            context_init,
            0 as usize,
            0 as usize,
            stack,
        );
        let coroutine = Coroutine {
            context: context,
            function: Some(function),
            event_loop_registration: None,
            state: CoroutineState::New,
        };

        coroutine
    }

    pub fn raw_pointer(&self) -> *const Coroutine {
        self.borrow() as *const Coroutine
    }

    pub fn run(&mut self, scheduler_context: &Context) -> Result<()> {
        try!(self.set_context_to_run_coroutine());
        try!(self.set_context_to_return_to_scheduler(scheduler_context));
        self.state = CoroutineState::Running;

        Context::swap(scheduler_context, &self.context);

        Ok(())
    }

    pub fn blocked(&self) -> bool {
        match self.state {
            CoroutineState::Blocked => true,
            _ => false,
        }
    }

    /// Moving ownership of a value can change it's memory address. The first
    /// argument to the context_init, the pointer back to the calling coroutine,
    /// needs to be updated before coroutine is run in case the coroutine has
    /// been moved.
    fn set_context_to_run_coroutine(&mut self) -> Result<()> {
        let raw_self_ptr = self.raw_pointer();

        try!(self.context.set_arg(raw_self_ptr as usize, 0));

        Ok(())
    }

    fn set_context_to_return_to_scheduler(&mut self, scheduler_context: &Context) -> Result<()> {
        let scheduler_context_ptr = scheduler_context.borrow() as *const Context;
        try!(self.context.set_arg(scheduler_context_ptr as usize, 1));

        Ok(())
    }
}

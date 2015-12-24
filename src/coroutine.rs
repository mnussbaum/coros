use std::borrow::Borrow;
use std::boxed::FnBox;
use std::mem;

use context::{
    Context,
    Stack,
};
use mio::EventLoop;

use coroutine_blocking_handle::CoroutineBlockingHandle;
use thread_scheduler::{
    BlockedCoroutineSlab,
    ThreadScheduler,
};

#[derive(Debug)]
pub enum CoroutineState {
    New,
    Running,
    Blocked,
    Terminated,
}

extern "C" fn context_init(coroutine_ptr: usize, scheduler_context_ptr: usize) -> ! {
    let coroutine: &mut Coroutine = unsafe { mem::transmute(coroutine_ptr) };
    let function = coroutine
        .function
        .take()
        .expect("Coros internal error: cannot run coroutine without function");
    let scheduler_context: &Context = unsafe { mem::transmute(scheduler_context_ptr) };
    let mut coroutine_blocking_handle = CoroutineBlockingHandle {
        coroutine: coroutine,
        scheduler_context: scheduler_context,
    };

    function.call_box((&mut coroutine_blocking_handle,));

    Context::load(&scheduler_context);

    unreachable!("Coros internal error: execution should never reach here");
}

pub type EventLoopRegistrationCallback = Box<FnBox(Coroutine, &mut EventLoop<ThreadScheduler>, &mut BlockedCoroutineSlab)>;

pub struct Coroutine {
    pub context: Option<Context>,
    function: Option<Box<FnBox(&mut CoroutineBlockingHandle) + Send + 'static>>,
    pub event_loop_registration: Option<EventLoopRegistrationCallback>,
    pub state: CoroutineState,
}

unsafe impl Send for Coroutine {}

impl Coroutine {
    pub fn new(
        function: Box<FnBox(&mut CoroutineBlockingHandle) + Send + 'static>,
        stack: Stack,
    ) -> Coroutine
    {
        let mut coroutine = Coroutine {
            context: None,
            function: Some(function),
            event_loop_registration: None,
            state: CoroutineState::New,
        };
        let context = Context::new(
            context_init,
            0 as usize,
            0 as usize,
            stack,
        );
        coroutine.context = Some(context);

        coroutine
    }

    pub fn raw_pointer(&self) -> *const Coroutine {
        self.borrow() as *const Coroutine
    }

    pub fn run(&mut self, scheduler_context: &Context) {
        self.set_context_to_run_coroutine();
        self.set_context_to_return_to_scheduler(scheduler_context);
        self.state = CoroutineState::Running;

        match self.context {
            None => panic!("Coros internal error: trying to run coroutine without context"),
            Some(ref context) => {
                Context::swap(scheduler_context, context);
            },
        }
    }

    pub fn terminated(&self) -> bool {
        match self.state {
            CoroutineState::Terminated => true,
            _ => false,
        }
    }

    /// Moving ownership of a value can change it's memory address. The first
    /// argument to the context_init, the pointer back to the calling coroutine,
    /// needs to be updated before coroutine is run in case the coroutine has
    /// been moved.
    fn set_context_to_run_coroutine(&mut self) {
        let raw_self_ptr = self.raw_pointer();

        match self.context {
            None => panic!("Coros internal error: trying to run coroutine without context"),
            Some(ref mut context) => {
                context
                    .set_arg(raw_self_ptr as usize, 0)
                    .expect("Coros internal error: trying to set arg on context without stack");
            },
        }
    }

    fn set_context_to_return_to_scheduler(&mut self, scheduler_context: &Context) {
        match self.context {
            None => panic!("Coros internal error: trying to run coroutine without context"),
            Some(ref mut context) => {
                let scheduler_context_ptr = scheduler_context.borrow() as *const Context;
                context
                    .set_arg(scheduler_context_ptr as usize, 1)
                    .expect("Coros internal error: trying to set arg on context without stack");
            },
        }
    }
}

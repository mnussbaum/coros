use std::borrow::Borrow;
use std::boxed::FnBox;
use std::mem;

use context::{
    Context,
    Stack,
};

use coroutine_handle::CoroutineHandle;

#[derive(Debug)]
pub enum CoroutineState {
    New,
    Runnable,
    Sleeping,
    Terminated,
}

extern "C" fn context_init(coroutine_ptr: usize, scheduler_context_ptr: usize) -> ! {
    let coroutine: &mut Coroutine = unsafe { mem::transmute(coroutine_ptr) };
    let function = coroutine
        .function
        .take()
        .expect("Coros internal error: cannot run coroutine without function");
    let scheduler_context: &Context = unsafe { mem::transmute(scheduler_context_ptr) };
    let mut coroutine_handle = CoroutineHandle {
        coroutine: coroutine,
        scheduler_context: scheduler_context,
    };

    function.call_box((&mut coroutine_handle,));

    Context::load(&scheduler_context);

    unreachable!("Coros internal error: execution should never reach here");
}


pub struct Coroutine {
    pub context: Option<Context>,
    function: Option<Box<FnBox(&mut CoroutineHandle) + Send + 'static>>,
    pub state: CoroutineState,
}

impl Coroutine {
    pub fn new(
        function: Box<FnBox(&mut CoroutineHandle) + Send + 'static>,
        stack: Stack,
    ) -> Coroutine
    {
        let mut coroutine = Coroutine {
            context: None,
            function: Some(function),
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
        self.point_context_at_coroutine();
        self.point_context_at_scheduler_contex(scheduler_context);
        self.state = CoroutineState::Runnable;

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

    pub fn take_stack(&mut self) -> Stack {
        match self.context {
            None => panic!("Coros internal error: trying to take stack from coroutine without context"),
            Some(ref mut context) => {
                context
                    .take_stack()
                    .expect("Coros internal error: trying to run release stack from coroutine without one")
            },
        }
    }

    /// Moving ownership of a value can change it's memory address. The first
    /// argument to the context_init, the pointer back to the calling coroutine,
    /// needs to be updated before coroutine is run in case the coroutine has
    /// been moved.
    fn point_context_at_coroutine(&mut self) {
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

    fn point_context_at_scheduler_contex(&mut self, scheduler_context: &Context) {
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

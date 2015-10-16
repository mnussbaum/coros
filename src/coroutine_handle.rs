use std::thread;

use context::Context;

use coroutine::{
    Coroutine,
    CoroutineState,
};

pub struct CoroutineHandle<'a> {
    pub coroutine: &'a mut Coroutine,
    pub scheduler_context: &'a Context,
}

impl<'a> CoroutineHandle<'a> {
    pub fn sleep_ms(&mut self, ms: u32) {
        self.coroutine.state = CoroutineState::Sleeping;
        match self.coroutine.context {
            Some(ref context) => {
                Context::swap(context, self.scheduler_context);
                thread::sleep_ms(ms); // Dirty hack until we get a real timer
            },
            None => panic!("Coros internal error: cannot sleep coroutine without context"),
        };
    }
}

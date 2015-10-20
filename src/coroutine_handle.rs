use std::sync::mpsc::RecvError;

use mio::util::Slab;
use mio::EventLoop;

use context::Context;

use coroutine::{
    Coroutine,
    CoroutineState,
};
use notifying_channel::{
    BlockedMessage,
    NotifyingReceiver,
};
use thread_scheduler::ThreadScheduler;

pub struct CoroutineHandle<'a> {
    pub coroutine: &'a mut Coroutine,
    pub scheduler_context: &'a Context,
}

impl<'a> CoroutineHandle<'a> {
    pub fn sleep_ms(&mut self, ms: u64) {
        self.coroutine.state = CoroutineState::Sleeping;
        self.coroutine.mio_callback = Some(Box::new(move |coroutine: Coroutine, mio_event_loop: &mut EventLoop<ThreadScheduler>, blocked_coroutines: &mut Slab<Coroutine>| {
            let token = blocked_coroutines
                .insert(coroutine)
                .ok()
                .expect("Coros internal error: error inserting coroutine into slab");

            mio_event_loop
                .timeout_ms(token, ms).
                expect("Coros internal error: ran out of slab");
        }));

        match self.coroutine.context {
            Some(ref context) => {
                Context::swap(context, self.scheduler_context);
            },
            None => panic!("Coros internal error: cannot sleep coroutine without context"),
        };
    }

    pub fn recv<M: Send>(&mut self, receiver: &NotifyingReceiver<M>) -> Result<M, RecvError> {
        let blocked_message_sender = receiver.blocked_message_sender.clone();
        self.coroutine.state = CoroutineState::Sleeping;
        self.coroutine.mio_callback = Some(Box::new(move |coroutine: Coroutine, mio_event_loop: &mut EventLoop<ThreadScheduler>, blocked_coroutines: &mut Slab<Coroutine>| {
            let token = blocked_coroutines
                .insert(coroutine)
                .ok()
                .expect("Coros internal error: error inserting coroutine into slab");
            let mio_sender = mio_event_loop.channel();
            let message = BlockedMessage {
                mio_sender: mio_sender,
                token: token,
            };
            blocked_message_sender.send(message).unwrap(); //TODO: handle errors, encapsulate
        }));

        match self.coroutine.context {
            Some(ref context) => {
                Context::swap(context, self.scheduler_context);
            },
            None => panic!("Coros internal error: cannot sleep coroutine without context"),
        };

        receiver.recv()
    }
}

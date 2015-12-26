use std::sync::mpsc::channel;

use mio::{
    EventLoop,
    EventSet,
    Evented,
    PollOpt,
};

use context::Context;

use coroutine::{
    EventLoopRegistrationCallback,
    Coroutine,
    CoroutineState,
};
use coroutine_channel::{
    BlockedMessage,
    CoroutineReceiver,
};
use error::CorosError;
use Result;
use thread_scheduler::{
    BlockedCoroutineSlab,
    ThreadScheduler,
};

pub struct CoroutineBlockingHandle<'a> {
    pub coroutine: &'a mut Coroutine,
    pub scheduler_context: &'a Context,
}

impl<'a> CoroutineBlockingHandle<'a> {
    pub fn sleep_ms(&mut self, ms: u64) -> Result<()> {
        self.coroutine.state = CoroutineState::Blocked;

        let mio_callback = move |coroutine: Coroutine,
                                 mio_event_loop: &mut EventLoop<ThreadScheduler>,
                                 blocked_coroutines: &mut BlockedCoroutineSlab| {
            let token = blocked_coroutines
                .insert((coroutine, None))
                .ok()
                .expect("Coros internal error: error inserting coroutine into slab");

            mio_event_loop
                .timeout_ms(token, ms)
                .expect("Coros internal error: ran out of slab");
        };

        self.suspend_with_callback(Box::new(mio_callback))
    }

    pub fn recv<M: Send>(&mut self, receiver: &CoroutineReceiver<M>) -> Result<M> {
        let blocked_message_sender = receiver.blocked_message_sender.clone();
        self.coroutine.state = CoroutineState::Blocked;

        let mio_callback = move |coroutine: Coroutine,
                                 mio_event_loop: &mut EventLoop<ThreadScheduler>,
                                 blocked_coroutines: &mut BlockedCoroutineSlab| {
            let token = blocked_coroutines
                .insert((coroutine, None))
                .ok()
                .expect("Coros internal error: error inserting coroutine into slab");
            let mio_sender = mio_event_loop.channel();
            let message = BlockedMessage {
                mio_sender: mio_sender,
                token: token,
            };
            blocked_message_sender.send(message).unwrap(); //TODO: handle errors, encapsulate
        };

        try!(self.suspend_with_callback(Box::new(mio_callback)));

        Ok(try!(receiver.recv()))
    }

    pub fn register<E: ?Sized>(&mut self, io: &E, interest: EventSet, opt: PollOpt) -> Result<EventSet>
        where E: Evented + 'static
    {
        self.coroutine.state = CoroutineState::Blocked;
        let (awoken_for_eventset_tx, awoken_for_eventset_rx) = channel::<EventSet>();
        let raw_io_ptr: *const E = io as *const E;

        let mio_callback = move |coroutine: Coroutine,
                                 mio_event_loop: &mut EventLoop<ThreadScheduler>,
                                 blocked_coroutines: &mut BlockedCoroutineSlab| {
            let token = blocked_coroutines
                .insert((coroutine, Some(awoken_for_eventset_tx)))
                .ok()
                .expect("Coros internal error: error inserting coroutine into slab");
            mio_event_loop.register(
                unsafe { &*raw_io_ptr },
                token,
                interest,
                opt,
            ).unwrap(); //TODO: error handling
        };

        try!(self.suspend_with_callback(Box::new(mio_callback)));

        Ok(try!(awoken_for_eventset_rx.recv()))
    }

    pub fn deregister<E: ?Sized>(&mut self, io: &E) -> Result<()>
        where E: Evented + 'static
    {
        self.coroutine.state = CoroutineState::Blocked;
        let raw_io_ptr: *const E = io as *const E;

        let mio_callback = move |coroutine: Coroutine,
                                 mio_event_loop: &mut EventLoop<ThreadScheduler>,
                                 blocked_coroutines: &mut BlockedCoroutineSlab| {
            let token = blocked_coroutines
                .insert((coroutine, None))
                .ok()
                .expect("Coros internal error: error inserting coroutine into slab");
            mio_event_loop.deregister(unsafe { &*raw_io_ptr }).unwrap(); //TODO: error handling
            mio_event_loop
                .timeout_ms(token, 0)
                .expect("Coros internal error: ran out of slab");
        };

        Ok(try!(self.suspend_with_callback(Box::new(mio_callback))))
    }

    pub fn reregister<E: ?Sized>(&mut self, io: &E, interest: EventSet, opt: PollOpt) -> Result<EventSet>
        where E: Evented + 'static
    {
        self.coroutine.state = CoroutineState::Blocked;
        let (awoken_for_eventset_tx, awoken_for_eventset_rx) = channel::<EventSet>();
        let raw_io_ptr: *const E = io as *const E;

        let mio_callback = move |coroutine: Coroutine,
                                 mio_event_loop: &mut EventLoop<ThreadScheduler>,
                                 blocked_coroutines: &mut BlockedCoroutineSlab| {
            let token = blocked_coroutines
                .insert((coroutine, Some(awoken_for_eventset_tx)))
                .ok()
                .expect("Coros internal error: error inserting coroutine into slab");
            mio_event_loop.reregister(
                unsafe { &*raw_io_ptr },
                token,
                interest,
                opt,
            ).unwrap(); //TODO: error handling
        };

        try!(self.suspend_with_callback(Box::new(mio_callback)));

        Ok(try!(awoken_for_eventset_rx.recv()))
    }

    fn suspend_with_callback(&mut self, event_loop_registration: EventLoopRegistrationCallback) -> Result<()> {
        self.coroutine.event_loop_registration = Some(event_loop_registration);

        match self.coroutine.context {
            Some(ref context) => {
                Context::swap(context, self.scheduler_context);

                Ok(())
            },
            None => Err(CorosError::InvalidCoroutineNoContext),
        }
    }
}

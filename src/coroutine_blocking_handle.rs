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
                                 blocked_coroutines: &mut BlockedCoroutineSlab| -> Result<()> {
            let token = match blocked_coroutines.insert((coroutine, None)) {
                Ok(token) => token,
                Err(_) => return Err(CorosError::SlabFullError),
            };

            try!(mio_event_loop.timeout_ms(token, ms));

            Ok(())
        };

        self.suspend_with_callback(Box::new(mio_callback))
    }

    pub fn recv<M: Send>(&mut self, rx: &CoroutineReceiver<M>) -> Result<M> {
        let blocked_message_tx = rx.blocked_message_tx.clone();
        self.coroutine.state = CoroutineState::Blocked;

        let mio_callback = move |coroutine: Coroutine,
                                 mio_event_loop: &mut EventLoop<ThreadScheduler>,
                                 blocked_coroutines: &mut BlockedCoroutineSlab| -> Result<()> {
            let token = match blocked_coroutines.insert((coroutine, None)) {
                Ok(token) => token,
                Err(_) => return Err(CorosError::SlabFullError),
            };
            let mio_tx = mio_event_loop.channel();
            let message = BlockedMessage {
                mio_tx: mio_tx,
                token: token,
            };

            if let Err(_) = blocked_message_tx.send(message) {
                return Err(CorosError::CoroutineBlockSendError)
            }

            Ok(())
        };

        try!(self.suspend_with_callback(Box::new(mio_callback)));

        Ok(try!(rx.recv()))
    }

    pub fn register<E: ?Sized>(&mut self, io: &E, interest: EventSet, opt: PollOpt) -> Result<EventSet>
        where E: Evented + 'static
    {
        self.coroutine.state = CoroutineState::Blocked;
        let (eventset_tx, eventset_rx) = channel::<EventSet>();
        let raw_io_ptr: *const E = io as *const E;

        let mio_callback = move |coroutine: Coroutine,
                                 mio_event_loop: &mut EventLoop<ThreadScheduler>,
                                 blocked_coroutines: &mut BlockedCoroutineSlab| -> Result<()> {
            let token = match blocked_coroutines.insert((coroutine, Some(eventset_tx))) {
                Ok(token) => token,
                Err(_) => return Err(CorosError::SlabFullError),
            };
            try!(
                mio_event_loop.register(
                    unsafe { &*raw_io_ptr },
                    token,
                    interest,
                    opt,
                )
            );

            Ok(())
        };

        try!(self.suspend_with_callback(Box::new(mio_callback)));

        Ok(try!(eventset_rx.recv()))
    }

    pub fn deregister<E: ?Sized>(&mut self, io: &E) -> Result<()>
        where E: Evented + 'static
    {
        self.coroutine.state = CoroutineState::Blocked;
        let raw_io_ptr: *const E = io as *const E;

        let mio_callback = move |coroutine: Coroutine,
                                 mio_event_loop: &mut EventLoop<ThreadScheduler>,
                                 blocked_coroutines: &mut BlockedCoroutineSlab| -> Result<()> {
            let token = match blocked_coroutines.insert((coroutine, None)) {
                Ok(token) => token,
                Err(_) => return Err(CorosError::SlabFullError),
            };
            try!(mio_event_loop.deregister(unsafe { &*raw_io_ptr }));
            try!(mio_event_loop.timeout_ms(token, 0));

            Ok(())
        };

        Ok(try!(self.suspend_with_callback(Box::new(mio_callback))))
    }

    pub fn reregister<E: ?Sized>(&mut self, io: &E, interest: EventSet, opt: PollOpt) -> Result<EventSet>
        where E: Evented + 'static
    {
        self.coroutine.state = CoroutineState::Blocked;
        let (eventset_tx, eventset_rx) = channel::<EventSet>();
        let raw_io_ptr: *const E = io as *const E;

        let mio_callback = move |coroutine: Coroutine,
                                 mio_event_loop: &mut EventLoop<ThreadScheduler>,
                                 blocked_coroutines: &mut BlockedCoroutineSlab| -> Result<()> {
            let token = match blocked_coroutines.insert((coroutine, Some(eventset_tx))) {
                Ok(token) => token,
                Err(_) => return Err(CorosError::SlabFullError),
            };
            try!(
                mio_event_loop.reregister(
                    unsafe { &*raw_io_ptr },
                    token,
                    interest,
                    opt,
                )
            );

            Ok(())
        };

        try!(self.suspend_with_callback(Box::new(mio_callback)));

        Ok(try!(eventset_rx.recv()))
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

use std::panic::{RecoverSafe, RefRecoverSafe};
use std::sync::mpsc::channel;
use std::sync::MutexGuard;
use std::time::Duration;

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
use coroutine::channel::{
    BlockedMessage,
    Receiver,
};
use error::CorosError;
use Result;
use scheduler::{
    BlockedCoroutineSlab,
    Scheduler,
};

pub struct IoHandle<'a> {
    pub coroutine: &'a mut Coroutine,
    pub scheduler_context: &'a Context,
}

impl<'a> IoHandle<'a> {
    pub fn sleep(&mut self, duration: Duration) -> Result<()> {
        self.coroutine.state = CoroutineState::Blocked;

        let mio_callback = move |coroutine: Coroutine,
                                 mio_event_loop: &mut EventLoop<Scheduler>,
                                 blocked_coroutines: &mut BlockedCoroutineSlab| -> Result<()> {
            let token = match blocked_coroutines.insert((coroutine, None)) {
                Ok(token) => token,
                Err(_) => return Err(CorosError::SlabFull),
            };

            try!(mio_event_loop.timeout(token, duration));

            Ok(())
        };

        self.suspend_with_callback(Box::new(mio_callback))
    }

    pub fn recv<M: Send>(&mut self, rx: &MutexGuard<Receiver<M>>) -> Result<M> {
        let blocked_message_tx = rx.blocked_message_tx.clone();
        self.coroutine.state = CoroutineState::Blocked;

        let mio_callback = move |coroutine: Coroutine,
                                 mio_event_loop: &mut EventLoop<Scheduler>,
                                 blocked_coroutines: &mut BlockedCoroutineSlab| -> Result<()> {
            let token = match blocked_coroutines.insert((coroutine, None)) {
                Ok(token) => token,
                Err(_) => return Err(CorosError::SlabFull),
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
                                 mio_event_loop: &mut EventLoop<Scheduler>,
                                 blocked_coroutines: &mut BlockedCoroutineSlab| -> Result<()> {
            let token = match blocked_coroutines.insert((coroutine, Some(eventset_tx))) {
                Ok(token) => token,
                Err(_) => return Err(CorosError::SlabFull),
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
                                 mio_event_loop: &mut EventLoop<Scheduler>,
                                 blocked_coroutines: &mut BlockedCoroutineSlab| -> Result<()> {
            let token = match blocked_coroutines.insert((coroutine, None)) {
                Ok(token) => token,
                Err(_) => return Err(CorosError::SlabFull),
            };
            try!(mio_event_loop.deregister(unsafe { &*raw_io_ptr }));
            try!(mio_event_loop.timeout(token, Duration::new(0, 0)));

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
                                 mio_event_loop: &mut EventLoop<Scheduler>,
                                 blocked_coroutines: &mut BlockedCoroutineSlab| -> Result<()> {
            let token = match blocked_coroutines.insert((coroutine, Some(eventset_tx))) {
                Ok(token) => token,
                Err(_) => return Err(CorosError::SlabFull),
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

        Context::swap(&self.coroutine.context, self.scheduler_context);

        Ok(())
    }
}

impl<'_> RecoverSafe for IoHandle<'_> {}
impl<'_> RefRecoverSafe for IoHandle<'_> {}

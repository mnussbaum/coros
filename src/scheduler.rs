use std::sync::mpsc::{
    Receiver,
    Sender,
    TryRecvError,
};
use std::sync::Mutex;
use std::time::Duration;

use context::Context;
use deque::{
    Stealer,
    Stolen,
    Worker,
};
use slab::Slab;
use mio::{
    EventLoop,
    EventSet,
    Token,
};
use mio::Handler as MioHandler;

use coroutine::Coroutine;
use error::CorosError;
use Result;

pub type BlockedCoroutineSlab = Slab<(Coroutine, Option<Sender<EventSet>>), Token>;

pub struct Scheduler {
    blocked_coroutines: BlockedCoroutineSlab,
    is_shutting_down: bool,
    mio_event_loop: EventLoop<Scheduler>,
    result_tx: Sender<Result<()>>,
    scheduler_context: Context,
    shutdown_rx: Receiver<()>,
    work_provider: Worker<Coroutine>,
    work_rx: Receiver<Coroutine>,
    work_stealers: Mutex<Vec<Stealer<Coroutine>>>,
}

impl MioHandler for Scheduler {
    type Timeout = Token;
    type Message = Token;

    fn notify(&mut self, _: &mut EventLoop<Scheduler>, coroutine_token: Token) {
        if let Err(err) = self.enqueue_coroutine(coroutine_token, None) {
          error!("Error notifying coroutine of IO: {:?}", err);
        }
    }

    fn ready(&mut self, _: &mut EventLoop<Scheduler>, coroutine_token: Token, eventset: EventSet) {
        if let Err(err) = self.enqueue_coroutine(coroutine_token, Some(eventset)) {
          error!("Error readying coroutine for IO: {:?}", err);
        }
    }

    fn timeout(&mut self, _: &mut EventLoop<Scheduler>, coroutine_token: Token) {
        if let Err(err) = self.enqueue_coroutine(coroutine_token, None) {
          error!("Error awakening coroutine after timer alert: {:?}", err);
        }
    }
}

const MAX_STOLEN_WORK_BATCH_SIZE: usize = 1000;

impl Scheduler {
    pub fn new(
        result_tx: Sender<Result<()>>,
        shutdown_rx: Receiver<()>,
        work_provider: Worker<Coroutine>,
        work_rx: Receiver<Coroutine>,
        work_stealers: Vec<Stealer<Coroutine>>,
    ) -> Result<Scheduler> {
        Ok(Scheduler {
            blocked_coroutines: Slab::new(1024 * 64),
            is_shutting_down: false,
            mio_event_loop: try!(EventLoop::new()),
            result_tx: result_tx,
            scheduler_context: Context::empty(),
            shutdown_rx: shutdown_rx,
            work_provider: work_provider,
            work_rx: work_rx,
            work_stealers: Mutex::new(work_stealers),
        })
    }

    fn run_coroutine(&mut self, coroutine: Coroutine) -> Result<()> {
        let mut coroutine = coroutine;
        try!(coroutine.run(&self.scheduler_context));

        if coroutine.blocked() {
            match coroutine.event_loop_registration.take() {
                Some(event_loop_registration) => try!(
                    event_loop_registration.call_box((
                        coroutine,
                        &mut self.mio_event_loop,
                        &mut self.blocked_coroutines
                    ))
                ),
                None => return Err(CorosError::InvalidCoroutineNoCallback),
            }
        }

        Ok(())
    }

    pub fn run(&mut self) {
        let result_tx = self.result_tx.clone();
        result_tx
            .send(self.run_eventloop())
            .expect("Coros internal error: attempting to send thread scheduler result to closed channel");
    }

    pub fn run_eventloop(&mut self) -> Result<()> {
        'event_loop:
        while !self.ready_to_shutdown() {
            try!(self.move_received_work_onto_queue());

            let raw_self_ptr: *mut Scheduler = self;
            let event_loop_tick_timeout = Some(Duration::from_millis(10));
            try!(self.mio_event_loop.run_once(
                    unsafe { &mut *raw_self_ptr },
                    event_loop_tick_timeout
            ));

            let work_result = match self.work_provider.pop() {
                Some(coroutine) => self.run_coroutine(coroutine),
                None => {
                    match try!(self.stolen_work()) {
                        Some(coroutine) => self.run_coroutine(coroutine),
                        None => Ok(()),
                    }
                },
            };

            try!(work_result);
        };

        Ok(())
    }

    fn ready_to_shutdown(&mut self) -> bool {
        if self.shutdown_rx.try_recv().is_ok() {
            self.is_shutting_down = true
        }

        self.is_shutting_down && self.blocked_coroutines.is_empty()
    }

    pub fn stolen_work(&mut self) -> Result<Option<Coroutine>> {
        let ref work_stealers = try!(self.work_stealers.lock());
        for work_stealer in work_stealers.iter() {
            if let Stolen::Data(coroutine) = work_stealer.steal() {
                return Ok(Some(coroutine));
            }
        }

        Ok(None)
    }

    pub fn move_received_work_onto_queue(&self) -> Result<()> {
        for _ in 0..MAX_STOLEN_WORK_BATCH_SIZE {
            match self.work_rx.try_recv() {
                Ok(work) => self.work_provider.push(work),
                Err(TryRecvError::Empty) => break,
                Err(err) => return Err(CorosError::from(err)),
            };
        }

        Ok(())
    }

    fn enqueue_coroutine(&mut self, coroutine_token: Token, maybe_eventset: Option<EventSet>) -> Result<()> {
        let blocked_on_io = try!(self.blocked_on_io(coroutine_token));
        let awoken_for_io = maybe_eventset.is_some();

        match (blocked_on_io, awoken_for_io) {
            (true, true) => {
                // Blocked on IO, awoken for IO

                let maybe_coroutine_slab_contents = self
                    .blocked_coroutines
                    .remove(coroutine_token);
                let (coroutine, maybe_eventset_tx) = match maybe_coroutine_slab_contents {
                    Some(coroutine_slab_contents) => coroutine_slab_contents,
                    None => return Err(CorosError::MissingCoroutine),
                };
                let eventset_tx = match maybe_eventset_tx {
                    Some(eventset_tx) => eventset_tx,
                    None => return Err(CorosError::InvalidCoroutineSlabContents),
                };
                let eventset = match maybe_eventset {
                    Some(eventset) => eventset,
                    None => panic!("Coros internal error: eventset changed in an impossible way"),
                };

                if let Err(_) = eventset_tx.send(eventset) {
                    return Err(CorosError::SendIoResultToCoroutineError)
                }

                self.work_provider.push(coroutine);
            },
            (true, false) => {
                // Blocked on IO, awoken for not-IO

                return Err(CorosError::CoroutineBlockedOnIoAwokenForNotIo)
            },
            (false, true) => {
                // Blocked on not-IO, awoken for IO

                // Nooping for now, but coroutines should have a prexisting eventset
                // queue where these can build up and can be returned from non-IO blocks
            },
            (false, false) => {
                // Blocked on not-IO, awoken for not-IO

                let maybe_coroutine_slab_contents = self
                    .blocked_coroutines
                    .remove(coroutine_token);
                let (coroutine, _) = match maybe_coroutine_slab_contents {
                    Some(coroutine_slab_contents) => coroutine_slab_contents,
                    None => return Err(CorosError::MissingCoroutine),
                };

                self.work_provider.push(coroutine);
            },
        };

        Ok(())
    }

    fn blocked_on_io(&self, coroutine_token: Token) -> Result<bool> {
        let maybe_coroutine_slab_contents = self.blocked_coroutines.get(coroutine_token);
        match maybe_coroutine_slab_contents {
            Some(&(_, ref maybe_eventset_tx)) => Ok(maybe_eventset_tx.is_some()),
            None => Err(CorosError::MissingCoroutine),
        }
    }
}

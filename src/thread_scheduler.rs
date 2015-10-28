use std::sync::mpsc::{
    Receiver,
    Sender,
    TryRecvError,
};
use std::sync::Mutex;

use context::Context;
use deque::{
    Stealer,
    Stolen,
    Worker,
};
use mio::util::Slab;
use mio::{
    EventLoop,
    EventSet,
    Token,
};
use mio::Handler as MioHandler;

use coroutine::Coroutine;

pub type BlockedCoroutineSlab = Slab<(Coroutine, Option<Sender<EventSet>>)>;

pub struct ThreadScheduler {
    blocked_coroutines: BlockedCoroutineSlab,
    mio_event_loop: EventLoop<ThreadScheduler>,
    scheduler_context: Context,
    shutdown_receiver: Receiver<()>,
    work_receiver: Receiver<Coroutine>,
    work_provider: Worker<Coroutine>,
    work_stealers: Mutex<Vec<Stealer<Coroutine>>>,
}

impl MioHandler for ThreadScheduler {
    type Timeout = Token;
    type Message = Token;

    fn notify(&mut self, _: &mut EventLoop<ThreadScheduler>, coroutine_token: Token) {
        self.enqueue_coroutine(coroutine_token, None);
    }

    fn ready(&mut self, _: &mut EventLoop<ThreadScheduler>, coroutine_token: Token, eventset: EventSet) {
        self.enqueue_coroutine(coroutine_token, Some(eventset));
    }

    fn timeout(&mut self, _: &mut EventLoop<ThreadScheduler>, coroutine_token: Token) {
        self.enqueue_coroutine(coroutine_token, None);
    }
}

impl ThreadScheduler {
    pub fn new(
        shutdown_receiver: Receiver<()>,
        work_provider: Worker<Coroutine>,
        work_receiver: Receiver<Coroutine>,
        work_stealers: Vec<Stealer<Coroutine>>,
    ) -> ThreadScheduler {
        ThreadScheduler {
            blocked_coroutines: Slab::new(1024 * 64),
            mio_event_loop: EventLoop::new().unwrap(),
            scheduler_context: Context::empty(),
            shutdown_receiver: shutdown_receiver,
            work_provider: work_provider,
            work_receiver: work_receiver,
            work_stealers: Mutex::new(work_stealers),
        }
    }

    fn run_coroutine(&mut self, coroutine: Coroutine) {
        let mut coroutine = coroutine;
        coroutine.run(&self.scheduler_context);
        if !coroutine.terminated() {
            let event_loop_registration = coroutine
                .event_loop_registration
                .take()
                .expect("Coros internal error: non-terminated state without callback");
            event_loop_registration.call_box((coroutine, &mut self.mio_event_loop, &mut self.blocked_coroutines));
        }
    }

    pub fn start(&mut self) {
        'event_loop:
        while self.shutdown_receiver.try_recv().is_err() {
            self.move_received_work_onto_queue();
            let raw_self_ptr: *mut ThreadScheduler = self;
            self.mio_event_loop
                .run_once(unsafe { &mut *raw_self_ptr })
                .ok()
                .expect("Coros internal error: error running mio event loop");

            match self.work_provider.pop() {
                Some(coroutine) => self.run_coroutine(coroutine),
                None => {
                    if let Some(coroutine) = self.stolen_work() {
                        self.run_coroutine(coroutine);
                    }
                },
            }
        };
    }

    pub fn stolen_work(&mut self) -> Option<Coroutine> {
        match self.work_stealers.lock() {
            Ok(ref work_stealers) => {
                for work_stealer in work_stealers.iter() {
                    if let Stolen::Data(coroutine) = work_stealer.steal() {
                        return Some(coroutine);
                    }
                }
            },
            Err(err) => panic!("Coros internal error: error getting work stealer lock: {}", err),
        }

        None
    }

    pub fn move_received_work_onto_queue(&self) {
        for _ in (0..1000) {
            match self.work_receiver.try_recv() {
                Ok(work) => self.work_provider.push(work),
                Err(TryRecvError::Empty) => break,
                Err(err) => panic!("Coros internal error: error receinving work {:?}", err),
            };
        }
    }

    fn enqueue_coroutine(&mut self, coroutine_token: Token, maybe_eventset: Option<EventSet>) {
        let blocked_on_io = self.blocked_on_io(coroutine_token);
        let awoken_for_io = maybe_eventset.is_some();

        match (blocked_on_io, awoken_for_io) {
            (true, true) => {
                // Blocked on IO, awoken for IO

                let (coroutine, maybe_awoken_for_eventset_rx) = self
                    .blocked_coroutines
                    .remove(coroutine_token)
                    .expect("Coros internal error: timeout expired for missing coroutine");
                let awoken_for_eventset_rx = maybe_awoken_for_eventset_rx.unwrap();
                let eventset = maybe_eventset.unwrap();

                awoken_for_eventset_rx.send(eventset).unwrap(); //TODO: error handling
                self.work_provider.push(coroutine);
            },
            (true, false) => {
                // Blocked on IO, awoken for not-IO

                panic!("Coros internal error: blocked on IO but awoken for not IO");
            },
            (false, true) => {
                // Blocked on not-IO, awoken for IO

                // Nooping for now, but coroutines should have a prexisting eventset
                // queue where these can build up and can be returned from non-IO blocks
            },
            (false, false) => {
                // Blocked on not-IO, awoken for not-IO

                let (coroutine, _) = self
                    .blocked_coroutines
                    .remove(coroutine_token)
                    .expect("Coros internal error: timeout expired for missing coroutine");
                self.work_provider.push(coroutine);
            },
        };
    }

    fn blocked_on_io(&self, coroutine_token: Token) -> bool {
        let &(_, ref maybe_awoken_for_eventset_rx) = self
            .blocked_coroutines
            .get(coroutine_token)
            .expect("Coros internal error: timeout expired for missing coroutine");

        maybe_awoken_for_eventset_rx.is_some()
    }
}

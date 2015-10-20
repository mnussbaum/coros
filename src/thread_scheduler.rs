use std::sync::mpsc::{
    Receiver,
    TryRecvError,
};
use std::sync::Mutex;
use std::thread;

use context::Context;
use deque::{
    Stealer,
    Stolen,
    Worker,
};
use mio::util::Slab;
use mio::{
    EventLoop,
    Token,
};
use mio::Handler as MioHandler;

use coroutine::Coroutine;

pub struct ThreadScheduler {
    blocked_coroutines: Slab<Coroutine>,
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

    fn notify(&mut self, _: &mut EventLoop<ThreadScheduler>, message_token: Token) {
        let coroutine = self
            .blocked_coroutines
            .remove(message_token)
            .expect("Coros internal error: received notification for missing coroutine");

        self.work_provider.push(coroutine);
    }

    fn timeout(&mut self, _: &mut EventLoop<ThreadScheduler>, timeout_token: Token) {
        let coroutine = self
            .blocked_coroutines
            .remove(timeout_token)
            .expect("Coros internal error: timeout expired for missing coroutine");

        self.work_provider.push(coroutine);
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
            let mio_callback = coroutine
                .mio_callback
                .take()
                .expect("Coros internal error: non-terminated state without callback");
            mio_callback.call_box((coroutine, &mut self.mio_event_loop, &mut self.blocked_coroutines));
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
                    } else {
                        thread::sleep_ms(100);
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
}

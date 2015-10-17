use std::sync::mpsc::{
    Receiver,
    TryRecvError,
};
use std::thread;

use context::Context;
use deque::{
    Stealer,
    Stolen,
    Worker,
};

use coroutine::Coroutine;

pub struct ThreadScheduler {
    scheduler_context: Context,
    shutdown_receiver: Receiver<()>,
    work_receiver: Receiver<Coroutine>,
    work_provider: Worker<Coroutine>,
    work_stealers: Vec<Stealer<Coroutine>>,
}

impl ThreadScheduler {
    pub fn new(
        shutdown_receiver: Receiver<()>,
        work_provider: Worker<Coroutine>,
        work_receiver: Receiver<Coroutine>,
        work_stealers: Vec<Stealer<Coroutine>>
    ) -> ThreadScheduler {
        ThreadScheduler {
            scheduler_context: Context::empty(),
            shutdown_receiver: shutdown_receiver,
            work_provider: work_provider,
            work_receiver: work_receiver,
            work_stealers: work_stealers,
        }
    }

    fn run_coroutine(&self, coroutine: Coroutine) {
        let mut coroutine = coroutine;
        coroutine.run(&self.scheduler_context);
        if !coroutine.terminated() {
            // Dirty hack until we get a real timer
            self.work_provider.push(coroutine);
        }
    }

    pub fn start(&mut self) {
        'event_loop:
        while self.shutdown_receiver.try_recv().is_err() {
            self.move_received_work_onto_queue();

            match self.work_provider.pop() {
                Some(coroutine) => self.run_coroutine(coroutine),
                None => {
                    for work_stealer in self.work_stealers.iter() {
                        if let Stolen::Data(coroutine) = work_stealer.steal() {
                            self.run_coroutine(coroutine);
                            continue 'event_loop;
                        }
                    }
                    thread::sleep_ms(100);
                },
            }
        };
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

use std::sync::atomic::{
    AtomicBool,
    Ordering,
};
use std::sync::Mutex;
use std::thread;

use context::Context;
use context::stack::{
    StackPool,
    Stack,
};
use deque::{
    Stealer,
    Stolen,
    Worker,
};

use coroutine::Coroutine;

pub struct ThreadScheduler {
    keep_running: AtomicBool,
    scheduler_context: Context,
    stack_pool: Mutex<StackPool>,
    work_provider: Worker<Coroutine>,
    work_stealers: Vec<Stealer<Coroutine>>,
}

impl ThreadScheduler {
    pub fn new(
        work_provider: Worker<Coroutine>,
        work_stealers: Vec<Stealer<Coroutine>>
    ) -> ThreadScheduler {
        ThreadScheduler {
            keep_running: AtomicBool::new(true),
            scheduler_context: Context::empty(),
            stack_pool: Mutex::new(StackPool::new()),
            work_provider: work_provider,
            work_stealers: work_stealers,
        }
    }

    fn run_coroutine(&self, coroutine: Coroutine) {
        let mut coroutine = coroutine;
        match self.stack_pool.lock() {
            Ok(mut stack_pool) => {
                coroutine.run(&self.scheduler_context);
                if coroutine.terminated() {
                    stack_pool.give_stack(coroutine.take_stack());
                } else {
                    // Dirty hack until we get a real timer
                    self.work_provider.push(coroutine);
                }
            },
            Err(e) => panic!("Coros internal error: cannot get stack mutex: {}", e),
        };
    }

    pub fn take_stack(&mut self, stack_size: usize) -> Stack {
        match self.stack_pool.lock() {
            Ok(mut stack_pool) => {
                stack_pool.take_stack(stack_size)
            },
            Err(e) => panic!("Coros internal error: cannot get stack mutex: {}", e),
        }
    }

    pub fn start(&self) {
        'eventloop:
        while self.keep_running.load(Ordering::SeqCst) {
            match self.work_provider.pop() {
                Some(coroutine) => self.run_coroutine(coroutine),
                None => {
                    for work_stealer in self.work_stealers.iter() {
                        if let Stolen::Data(coroutine) = work_stealer.steal() {
                            self.run_coroutine(coroutine);
                            continue 'eventloop;
                        }
                    }
                    thread::sleep_ms(100);
                },
            }
        };
    }

    pub fn stop(&self) {
        self.keep_running.store(false, Ordering::SeqCst);
    }

    pub fn send(&self, work: Coroutine) {
        self.work_provider.push(work);
    }
}

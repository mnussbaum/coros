use std::sync::atomic::{
    AtomicBool,
    Ordering,
};
use std::thread;

use context::{
    Context,
    Stack,
};
use deque::{
    Stealer,
    Stolen,
    Worker,
};

use coroutine::Coroutine;
use CoroutineWork;

pub struct ThreadScheduler {
    keep_running: AtomicBool,
    work_provider: Worker<CoroutineWork>,
    work_stealers: Vec<Stealer<CoroutineWork>>,
}

impl ThreadScheduler {
    pub fn new(work_provider: Worker<CoroutineWork>,
               work_stealers: Vec<Stealer<CoroutineWork>>) -> ThreadScheduler {
        ThreadScheduler {
            keep_running: AtomicBool::new(true),
            work_provider: work_provider,
            work_stealers: work_stealers,
        }
    }

    fn execute_work(&self, work: CoroutineWork) {
        let mut empty_context = Context::empty();
        let mut stack = Stack::new(2 * 1024 * 1024);
        let coroutine = Coroutine::new(work, &empty_context, &mut stack);
        Context::swap(&mut empty_context, &coroutine.context);
    }

    pub fn start(&self) {
        'eventloop:
        while self.keep_running.load(Ordering::SeqCst) {
            match self.work_provider.pop() {
                Some(work) => self.execute_work(work),
                None => {
                    for work_stealer in self.work_stealers.iter() {
                        if let Stolen::Data(work) = work_stealer.steal() {
                            self.execute_work(work);
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

    pub fn send(&self, work: CoroutineWork) {
        self.work_provider.push(work);
    }
}

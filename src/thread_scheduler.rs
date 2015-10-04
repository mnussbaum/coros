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
    BufferPool,
    Stealer,
    Worker,
};

use coroutine::Coroutine;

pub struct ThreadScheduler {
    keep_running: AtomicBool,
    work_receiver: Worker<Box<FnOnce() + Send + 'static>>,
    work_sender: Stealer<Box<FnOnce() + Send + 'static>>,
}

impl ThreadScheduler {
    pub fn new() -> ThreadScheduler {
        let (work_receiver, work_sender) = BufferPool::new().deque();
        ThreadScheduler {
            keep_running: AtomicBool::new(true),
            work_receiver: work_receiver,
            work_sender: work_sender,
        }
    }

    pub fn start(&self) {
        while self.keep_running.load(Ordering::SeqCst) {
            match self.work_receiver.pop() {
                Some(work) => {
                    let mut empty_context = Context::empty();
                    let mut stack = Stack::new(2 * 1024 * 1024);
                    let coroutine = Coroutine::new(work, &empty_context, &mut stack);
                    Context::swap(&mut empty_context, &coroutine.context);
                }
                None => thread::sleep_ms(100),
            }
        };
    }

    pub fn stop(&self) {
        self.keep_running.store(false, Ordering::SeqCst);
    }

    pub fn send(&self, work: Box<FnOnce() + Send + 'static>) {
        self.work_receiver.push(work);
    }
}

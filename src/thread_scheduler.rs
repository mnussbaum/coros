use deque::{
    BufferPool,
    Stealer,
    Worker,
};

pub struct ThreadScheduler {
    work_receiver: Worker<Box<FnOnce() + Send + 'static>>,
    work_sender: Stealer<Box<FnOnce() + Send + 'static>>,
}

impl ThreadScheduler {
    pub fn new() -> ThreadScheduler {
        let (work_receiver, work_sender) = BufferPool::new().deque();
        ThreadScheduler {
            work_receiver: work_receiver,
            work_sender: work_sender,
        }
    }

    pub fn start(&self) {
        loop {
            match self.work_receiver.pop() {
                Some(work) => work(),
                None => (),
            }
        };
    }

    pub fn send(&self, work: Box<FnOnce() + Send + 'static>) {
        self.work_receiver.push(work);
    }
}

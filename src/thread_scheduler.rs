use deque::{
    BufferPool,
    Stealer,
    Worker,
};

pub struct ThreadScheduler {
    id: u32,
    work_receiver: Worker<u32>,
    work_sender: Stealer<u32>,
}

impl ThreadScheduler {
    pub fn new() -> ThreadScheduler {
        let (work_receiver, work_sender) = BufferPool::new().deque();
        ThreadScheduler {
            id: 1,
            work_receiver: work_receiver,
            work_sender: work_sender,
        }
    }

    pub fn start(&self) {
        loop {};
    }
}

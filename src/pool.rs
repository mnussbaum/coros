use std::fmt;
use std::sync::RwLock;
use std::sync::mpsc::{
    channel,
    Receiver,
    Sender,
};

use context::stack::{
    Stack,
};
use deque::{
    BufferPool,
    Stealer,
    Worker,
};
use rand::{
    Rng,
    thread_rng,
};
use scoped_threadpool::Pool as ThreadPool;

use CoroutineJoinHandle;
use Result;
use coroutine::{
    Coroutine,
    CoroutineState,
};
use coroutine_blocking_handle::CoroutineBlockingHandle;
use error::CorosError;
use thread_scheduler::ThreadScheduler;

struct ThreadSchedulerComponents {
    shutdown_senders: Vec<Sender<()>>,
    result_receiver: Receiver<()>,
    result_sender: Sender<()>,
    thread_schedulers: Vec<ThreadScheduler>,
    work_senders: Vec<Sender<Coroutine>>,
}

pub struct Pool {
    pub is_running: bool,
    pub name: String,
    shutdown_senders: Vec<Sender<()>>,
    thread_count: u32,
    thread_pool: RwLock<ThreadPool>,
    thread_scheduler_result_receiver: Receiver<()>,
    thread_scheduler_result_sender: Sender<()>,
    thread_schedulers: Option<Vec<ThreadScheduler>>,
    work_senders: Vec<Sender<Coroutine>>,
}

impl fmt::Display for Pool {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Pool {} with {} threads", self.name, self.thread_count)
    }
}

impl fmt::Debug for Pool {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self)
    }
}

impl Pool {
    pub fn new(name: String, thread_count: u32) -> Pool {
        let thread_scheduler_components = Pool::create_thread_schedulers(thread_count);
        Pool {
            is_running: false,
            name: name,
            shutdown_senders: thread_scheduler_components.shutdown_senders,
            thread_count: thread_count,
            thread_pool: RwLock::new(ThreadPool::new(thread_count)),
            thread_scheduler_result_receiver: thread_scheduler_components.result_receiver,
            thread_scheduler_result_sender: thread_scheduler_components.result_sender,
            thread_schedulers: Some(thread_scheduler_components.thread_schedulers),
            work_senders: thread_scheduler_components.work_senders,
        }
    }

    fn create_thread_schedulers(thread_count: u32) -> ThreadSchedulerComponents {
        let thread_count: usize = thread_count as usize;
        let mut thread_schedulers: Vec<ThreadScheduler> = Vec::with_capacity(thread_count);
        let mut work_stealers: Vec<Stealer<Coroutine>> = Vec::with_capacity(thread_count);
        let mut work_providers: Vec<Worker<Coroutine>> = Vec::with_capacity(thread_count);
        let mut work_senders: Vec<Sender<Coroutine>> = Vec::with_capacity(thread_count);
        let mut shutdown_senders: Vec<Sender<()>> = Vec::with_capacity(thread_count);

        for _ in (0..thread_count) {
            let (work_provider, work_stealer) = BufferPool::<Coroutine>::new().deque();
            work_providers.push(work_provider);
            work_stealers.push(work_stealer);
        }
        for work_provider in work_providers.into_iter() {
            let (shutdown_sender, shutdown_receiver) = channel();
            shutdown_senders.push(shutdown_sender);
            let (work_sender, work_receiver) = channel();
            work_senders.push(work_sender);

            let thread_scheduler = ThreadScheduler::new(
                shutdown_receiver,
                work_provider,
                work_receiver,
                work_stealers.clone(),
            );
            thread_schedulers.push(thread_scheduler);
        }

        let (result_sender, result_receiver) = channel();
        ThreadSchedulerComponents {
            result_receiver: result_receiver,
            result_sender: result_sender,
            shutdown_senders: shutdown_senders,
            thread_schedulers: thread_schedulers,
            work_senders: work_senders,
        }
    }

    pub fn spawn_with_thread_index<F, T>(
        &mut self,
        coroutine_body: F,
        stack_size: usize,
        thread_index: u32
    ) -> CoroutineJoinHandle<T>
        where F: FnOnce(&mut CoroutineBlockingHandle) -> T + Send + 'static,
              T: Send + 'static,
    {
        let error_message = format!(
            "Corors error: cannot spawn in thread {} in a pool of size {}",
            thread_index,
            self.thread_count,
        );
        let work_sender = self.work_senders
            .get(thread_index as usize)
            .expect(&error_message[..]);

        let (coroutine_result_sender, coroutine_result_receiver) = channel();
        let coroutine_function = Box::new(move |coroutine_handle: &mut CoroutineBlockingHandle| {
            let maybe_coroutine_result = coroutine_body(coroutine_handle);

            coroutine_handle.coroutine.state = CoroutineState::Terminated;
            coroutine_result_sender
                .send(maybe_coroutine_result)
                .expect("Coros error: attempting to send coroutine result to closed channel");
        });

        let coroutine = Coroutine::new(
            coroutine_function,
            Stack::new(stack_size),
        );

        work_sender
            .send(coroutine)
            .expect("Coros error: attempting to send work to closed channel");

        CoroutineJoinHandle::<T>::new(coroutine_result_receiver)
    }

    pub fn spawn<F, T>(&mut self, coroutine_body: F, stack_size: usize) -> CoroutineJoinHandle<T>
        where F: FnOnce(&mut CoroutineBlockingHandle) -> T + Send + 'static,
              T: Send + 'static,
    {
        let thread_count = self.thread_count;
        self.spawn_with_thread_index(
            coroutine_body,
            stack_size,
            thread_rng().gen_range(0, thread_count) as u32,
        )
    }


    pub fn start(&mut self) -> Result<()> {
        if self.is_running {
            return Ok(())
        }

        let mut thread_pool =  try!(self.thread_pool.write());
        let mut thread_schedulers = self
            .thread_schedulers
            .take()
            .expect("Coros internal error: trying to start pool without schedulers");
        thread_pool.scoped(|scoped| {
            for mut thread_scheduler in thread_schedulers.drain(..) {
                let thread_scheduler_result_sender = self.thread_scheduler_result_sender.clone();
                scoped.execute(move || {
                    let scheduler_result = thread_scheduler.run();
                    thread_scheduler_result_sender
                        .send(scheduler_result)
                        .unwrap();
                });
            }
        });
        self.is_running = true;

        Ok(())
    }

    pub fn stop(&mut self) -> Result<Vec<Result<()>>> {
        if !self.is_running {
            return Ok(Vec::with_capacity(0));
        }

        let thread_pool =  try!(self.thread_pool.write());
        for shutdown_sender in self.shutdown_senders.drain(..) {
            shutdown_sender
                .send(())
                .expect("Coros internal error: error shutting down threads");
        }
        thread_pool.join_all();

        let mut results = Vec::with_capacity(self.thread_count as usize);
        for _ in (0..self.thread_count) {
            match self.thread_scheduler_result_receiver.recv() {
                Ok(result) => results.push(Ok(result)),
                Err(e) => results.push(Err(CorosError::from(e))),
            };
        }
        self.is_running = false;

        Ok(results)
    }
}

impl Drop for Pool {
    fn drop(&mut self) {
        self.stop().unwrap();
    }
}

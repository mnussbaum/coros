use std::fmt;
use std::sync::RwLock;
use std::sync::mpsc::channel;
use std::thread;

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
use CoroutineWork;
use Result;
use thread_scheduler::ThreadScheduler;

pub struct Pool {
    pub name: String,
    thread_pool: RwLock<ThreadPool>,
    thread_schedulers: Vec<ThreadScheduler>,
}

impl fmt::Display for Pool {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self.thread_pool.try_read() {
            Ok(thread_pool) => {
                write!(
                    f,
                    "Pool {} with {} threads",
                    self.name,
                    thread_pool.thread_count(),
                )
            },
            Err(_) => write!(f, "Pool {}", self.name),
        }
    }
}

impl fmt::Debug for Pool {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self)
    }
}

impl Pool {
    pub fn new(name: String, thread_count: u32) -> Pool {
        let thread_schedulers = Pool::create_thread_schedulers(thread_count);
        Pool {
            name: name,
            thread_pool: RwLock::new(ThreadPool::new(thread_count)),
            thread_schedulers: thread_schedulers,
        }
    }

    fn create_thread_schedulers(thread_count: u32) -> Vec<ThreadScheduler> {
        let thread_count: usize = thread_count as usize;
        let mut work_stealers: Vec<Stealer<CoroutineWork>> = Vec::with_capacity(thread_count);
        let mut work_providers: Vec<Worker<CoroutineWork>> = Vec::with_capacity(thread_count);
        for _ in (0..thread_count) {
            let (work_provider, work_stealer) = BufferPool::<CoroutineWork>::new().deque();
            work_providers.push(work_provider);
            work_stealers.push(work_stealer);
        }

        let mut thread_schedulers: Vec<ThreadScheduler> = Vec::with_capacity(thread_count);
        for work_provider in work_providers.into_iter() {
            thread_schedulers.push(ThreadScheduler::new(work_provider, work_stealers.clone()));
        }

        thread_schedulers
    }

    fn spawn_with_thread<F, T>(&self, coroutine_body: F, worker_thread: &ThreadScheduler) -> CoroutineJoinHandle<T>
        where F: FnOnce() -> T + Send + 'static,
              T: Send + 'static
    {
        let (coroutine_result_sender, coroutine_result_receiver) = channel();

        worker_thread.send(Box::new(move || {
            let maybe_coroutine_result = thread::catch_panic(move || {
                coroutine_body()
            });

            coroutine_result_sender
                .send(maybe_coroutine_result)
                .expect("Error attempting to send coroutine result to closed channel");
        }));

        CoroutineJoinHandle::<T>::new(coroutine_result_receiver)
    }

    pub fn spawn<F, T>(&self, coroutine_body: F) -> CoroutineJoinHandle<T>
        where F: FnOnce() -> T + Send + 'static,
              T: Send + 'static
    {
        let worker_thread = thread_rng()
            .choose(&self.thread_schedulers)
            .expect("Cannot spawn threads on uninitialized pool");

        self.spawn_with_thread(coroutine_body, worker_thread)
    }

    pub fn spawn_with_thread_index<F, T>(&self, coroutine_body: F, thread_index: u32) -> CoroutineJoinHandle<T>
        where F: FnOnce() -> T + Send + 'static,
              T: Send + 'static
    {
        let error_message = format!(
            "Cannot spawn in thread {} in a pool of size {}",
            thread_index,
            self.thread_schedulers.len(),
        );
        let worker_thread = &self.thread_schedulers
            .get(thread_index as usize)
            .expect(&error_message[..]);

        self.spawn_with_thread(coroutine_body, worker_thread)
    }

    pub fn start(&self) -> Result<()> {
        let mut thread_pool =  try!(self.thread_pool.write());
        thread_pool.scoped(|scoped| {
            for thread_scheduler in self.thread_schedulers.iter() {
                scoped.execute(move || { thread_scheduler.start(); () });
            }
        });

        Ok(())
    }

    pub fn stop(&self) -> Result<()> {
        let thread_pool =  try!(self.thread_pool.write());
        for thread_scheduler in self.thread_schedulers.iter() {
            thread_scheduler.stop();
        }
        thread_pool.join_all();
        Ok(())
    }
}

impl Drop for Pool {
    fn drop(&mut self) {
        self.stop().unwrap();
    }
}

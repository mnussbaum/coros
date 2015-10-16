use std::fmt;
use std::sync::RwLock;
use std::sync::mpsc::channel;

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
use coroutine_handle::CoroutineHandle;
use thread_scheduler::ThreadScheduler;

pub struct Pool {
    pub name: String,
    thread_count: u32,
    thread_pool: RwLock<ThreadPool>,
    thread_schedulers: Vec<ThreadScheduler>,
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
        let thread_schedulers = Pool::create_thread_schedulers(thread_count);
        Pool {
            name: name,
            thread_count: thread_count,
            thread_pool: RwLock::new(ThreadPool::new(thread_count)),
            thread_schedulers: thread_schedulers,
        }
    }

    fn create_thread_schedulers(thread_count: u32) -> Vec<ThreadScheduler> {
        let thread_count: usize = thread_count as usize;
        let mut work_stealers: Vec<Stealer<Coroutine>> = Vec::with_capacity(thread_count);
        let mut work_providers: Vec<Worker<Coroutine>> = Vec::with_capacity(thread_count);
        for _ in (0..thread_count) {
            let (work_provider, work_stealer) = BufferPool::<Coroutine>::new().deque();
            work_providers.push(work_provider);
            work_stealers.push(work_stealer);
        }

        let mut thread_schedulers: Vec<ThreadScheduler> = Vec::with_capacity(thread_count);
        for work_provider in work_providers.into_iter() {
            thread_schedulers.push(ThreadScheduler::new(work_provider, work_stealers.clone()));
        }

        thread_schedulers
    }

    pub fn spawn_with_thread_index<F, T>(
        &mut self,
        coroutine_body: F,
        stack_size: usize,
        thread_index: u32
    ) -> CoroutineJoinHandle<T>
        where F: FnOnce(&mut CoroutineHandle) -> T + Send + 'static,
              T: Send + 'static,
    {
        let error_message = format!(
            "Corors error: cannot spawn in thread {} in a pool of size {}",
            thread_index,
            self.thread_count,
        );
        let worker_thread = self.thread_schedulers
            .get_mut(thread_index as usize)
            .expect(&error_message[..]);

        let (coroutine_result_sender, coroutine_result_receiver) = channel();
        let coroutine_function = Box::new(move |coroutine_handle: &mut CoroutineHandle| {
            let maybe_coroutine_result = coroutine_body(coroutine_handle);

            coroutine_handle.coroutine.state = CoroutineState::Terminated;
            coroutine_result_sender
                .send(maybe_coroutine_result)
                .expect("Coros error: attempting to send coroutine result to closed channel");
        });

        let coroutine = Coroutine::new(
            coroutine_function,
            worker_thread.take_stack(stack_size),
        );

        worker_thread.send(coroutine);

        CoroutineJoinHandle::<T>::new(coroutine_result_receiver)
    }

    pub fn spawn<F, T>(&mut self, coroutine_body: F, stack_size: usize) -> CoroutineJoinHandle<T>
        where F: FnOnce(&mut CoroutineHandle) -> T + Send + 'static,
              T: Send + 'static,
    {
        let thread_count = self.thread_count;
        self.spawn_with_thread_index(
            coroutine_body,
            stack_size,
            thread_rng().gen_range(0, thread_count) as u32,
        )
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

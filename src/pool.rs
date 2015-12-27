use std::collections::hash_map::HashMap;
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
    shutdown_txs: Vec<Sender<()>>,
    result_rx: Receiver<Result<()>>,
    result_tx: Sender<Result<()>>,
    thread_schedulers: Vec<ThreadScheduler>,
    work_txs: Vec<Sender<Coroutine>>,
}

pub struct Pool {
    pub is_running: bool,
    pub name: String,
    shutdown_txs: Vec<Sender<()>>,
    thread_count: u32,
    thread_pool: RwLock<ThreadPool>,
    thread_scheduler_result_rx: Receiver<Result<()>>,
    thread_scheduler_result_tx: Sender<Result<()>>,
    thread_schedulers: Option<Vec<ThreadScheduler>>,
    work_txs: Vec<Sender<Coroutine>>,
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
    pub fn new(name: String, thread_count: u32) -> Result<Pool> {
        let thread_scheduler_components = try!(Pool::create_thread_schedulers(thread_count));
        Ok(Pool {
            is_running: false,
            name: name,
            shutdown_txs: thread_scheduler_components.shutdown_txs,
            thread_count: thread_count,
            thread_pool: RwLock::new(ThreadPool::new(thread_count)),
            thread_scheduler_result_rx: thread_scheduler_components.result_rx,
            thread_scheduler_result_tx: thread_scheduler_components.result_tx,
            thread_schedulers: Some(thread_scheduler_components.thread_schedulers),
            work_txs: thread_scheduler_components.work_txs,
        })
    }

    fn create_thread_schedulers(thread_count: u32) -> Result<ThreadSchedulerComponents> {
        let thread_count: usize = thread_count as usize;
        let mut thread_schedulers: Vec<ThreadScheduler> = Vec::with_capacity(thread_count);
        let mut work_stealers: Vec<Stealer<Coroutine>> = Vec::with_capacity(thread_count);
        let mut work_providers: Vec<Worker<Coroutine>> = Vec::with_capacity(thread_count);
        let mut work_txs: Vec<Sender<Coroutine>> = Vec::with_capacity(thread_count);
        let mut shutdown_txs: Vec<Sender<()>> = Vec::with_capacity(thread_count);

        for _ in 0..thread_count {
            let (work_provider, work_stealer) = BufferPool::<Coroutine>::new().deque();
            work_providers.push(work_provider);
            work_stealers.push(work_stealer);
        }
        for work_provider in work_providers.into_iter() {
            let (shutdown_tx, shutdown_rx) = channel();
            shutdown_txs.push(shutdown_tx);
            let (work_tx, work_rx) = channel();
            work_txs.push(work_tx);

            let thread_scheduler = try!(ThreadScheduler::new(
                shutdown_rx,
                work_provider,
                work_rx,
                work_stealers.clone(),
            ));
            thread_schedulers.push(thread_scheduler);
        }

        let (result_tx, result_rx) = channel();
        Ok(ThreadSchedulerComponents {
            result_rx: result_rx,
            result_tx: result_tx,
            shutdown_txs: shutdown_txs,
            thread_schedulers: thread_schedulers,
            work_txs: work_txs,
        })
    }

    pub fn spawn_with_thread_index<F, T>(
        &mut self,
        coroutine_body: F,
        stack_size: usize,
        thread_index: u32
    ) -> Result<CoroutineJoinHandle<T>>
        where F: FnOnce(&mut CoroutineBlockingHandle) -> T + Send + 'static,
              T: Send + 'static,
    {
        let work_tx = match self.work_txs.get(thread_index as usize) {
            None => {
                return Err(CorosError::InvalidThreadForSpawn(thread_index, self.thread_count))
            }
            Some(work_tx) => work_tx,
        };

        let (coroutine_result_tx, coroutine_result_rx) = channel();
        let coroutine_function = Box::new(move |coroutine_handle: &mut CoroutineBlockingHandle| {
            let maybe_coroutine_result = coroutine_body(coroutine_handle);

            coroutine_handle.coroutine.state = CoroutineState::Terminated;
            coroutine_result_tx
                .send(maybe_coroutine_result)
                .expect("Coros internal error: attempting to send coroutine result to closed channel");
        });

        let coroutine = Coroutine::new(
            coroutine_function,
            Stack::new(stack_size),
        );

        if let Err(_) = work_tx.send(coroutine) {
          return Err(CorosError::TriedToSpawnCoroutineOnShutdownThread)
        }

        Ok(CoroutineJoinHandle::<T>::new(coroutine_result_rx))
    }

    pub fn spawn<F, T>(&mut self, coroutine_body: F, stack_size: usize) -> Result<CoroutineJoinHandle<T>>
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
        let mut thread_schedulers = match self.thread_schedulers.take() {
            Some(thread_schedulers) => thread_schedulers,
            None => return Err(CorosError::CannotStartPoolWithoutSchedulers),
        };
        thread_pool.scoped(|scoped| {
            for mut thread_scheduler in thread_schedulers.drain(..) {
                let thread_scheduler_result_tx = self.thread_scheduler_result_tx.clone();
                scoped.execute(move || {
                    let scheduler_result = thread_scheduler.run();
                    thread_scheduler_result_tx
                        .send(scheduler_result)
                        .expect("Coros internal error: attempting to send thread scheduler result to closed channel");
                });
            }
        });
        self.is_running = true;

        Ok(())
    }

    pub fn stop(&mut self) -> Result<()> {
        if !self.is_running {
            return Ok(());
        }
        let mut errors = HashMap::new();
        let mut error_index = 0;

        let thread_pool =  try!(self.thread_pool.write());
        for shutdown_tx in self.shutdown_txs.drain(..) {
            if let Err(_) = shutdown_tx.send(()) {
                errors.insert(error_index, CorosError::UnableToSendThreadShutdownSignal);
            }
            error_index = error_index + 1;
        }
        thread_pool.join_all();

        for thread_index in 0..self.thread_count {
            if let Err(err) = self.thread_scheduler_result_rx.recv() {
                errors.insert(thread_index, CorosError::UnableToReceiveThreadShutdownResult(err));
            }
        }
        self.is_running = false;

        if errors.is_empty() {
            Ok(())
        } else {
            Err(CorosError::UncleanShutdown(errors))
        }
    }
}

impl Drop for Pool {
    fn drop(&mut self) {
        self.stop().unwrap();
    }
}

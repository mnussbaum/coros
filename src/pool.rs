use std::{fmt, panic};
use std::sync::{
    Mutex,
    RwLock,
};
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

use Result;
use coroutine::{
    Coroutine,
};
use coroutine::io_handle::IoHandle;
use coroutine::join_handle::JoinHandle;
use error::CorosError;
use scheduler::Scheduler;

struct SchedulerHandle {
    shutdown_tx: Sender<()>,
    scheduler: Mutex<Option<Scheduler>>,
    work_tx: Sender<Coroutine>,
}

pub struct Pool {
    pub is_running: bool,
    pub name: String,
    thread_count: u32,
    thread_pool: RwLock<ThreadPool>,
    scheduler_result_rx: Option<Receiver<Result<()>>>,
    scheduler_handles: Option<Vec<SchedulerHandle>>,
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
        let mut pool = Pool {
            is_running: false,
            name: name,
            thread_count: thread_count,
            thread_pool: RwLock::new(ThreadPool::new(thread_count)),
            scheduler_result_rx: None,
            scheduler_handles: None,
        };
        try!(pool.create_scheduler_handles());

        Ok(pool)
    }

    pub fn create_scheduler_handles(&mut self) -> Result<()> {
        let thread_count: usize = self.thread_count as usize;
        let mut scheduler_handles: Vec<SchedulerHandle> = Vec::with_capacity(thread_count);
        let mut work_stealers: Vec<Stealer<Coroutine>> = Vec::with_capacity(thread_count);
        let mut work_providers: Vec<Worker<Coroutine>> = Vec::with_capacity(thread_count);
        let (result_tx, result_rx) = channel();

        for _ in 0..thread_count {
            let (work_provider, work_stealer) = BufferPool::<Coroutine>::new().deque();
            work_providers.push(work_provider);
            work_stealers.push(work_stealer);
        }
        for work_provider in work_providers.into_iter() {
            let (shutdown_tx, shutdown_rx) = channel();
            let (work_tx, work_rx) = channel();

            let scheduler = try!(Scheduler::new(
                result_tx.clone(),
                shutdown_rx,
                work_provider,
                work_rx,
                work_stealers.clone(),
            ));
            let scheduler_handle = SchedulerHandle {
                shutdown_tx: shutdown_tx,
                scheduler: Mutex::new(Some(scheduler)),
                work_tx: work_tx,
            };
            scheduler_handles.push(scheduler_handle);
        }

        self.scheduler_result_rx = Some(result_rx);
        self.scheduler_handles = Some(scheduler_handles);

        Ok(())
    }

    pub fn spawn_with_thread_index<F, T>(
        &mut self,
        coroutine_body: F,
        stack_size: usize,
        thread_index: u32
    ) -> Result<JoinHandle<T>>
        where F: FnOnce(IoHandle) -> T + panic::RecoverSafe + Send + 'static,
              T: Send + 'static,
    {
        let scheduler_handles = match self.scheduler_handles {
            Some(ref scheduler_handles) => scheduler_handles,
            None => return Err(CorosError::CannotStartPoolWithoutSchedulers),
        };

        let scheduler_handle = match scheduler_handles.get(thread_index as usize) {
            None => {
                return Err(CorosError::InvalidThreadForSpawn(thread_index, self.thread_count))
            }
            Some(scheduler_handle) => scheduler_handle,
        };

        let (coroutine_result_tx, coroutine_result_rx) = channel();
        let coroutine_function = Box::new(move |coroutine_handle: IoHandle| {
            let maybe_coroutine_result = panic::recover(move || {
                coroutine_body(coroutine_handle)
            });

            let result = match maybe_coroutine_result {
                Ok(coroutine_result) => Ok(coroutine_result),
                Err(err) => {
                    error!("Coroutine body panicked with: {:?}", err);
                    Err(CorosError::CoroutinePanic)
                },
            };
            coroutine_result_tx
                .send(result)
                .expect("Coros internal error: attempting to send coroutine result to closed channel");
        });

        let coroutine = Coroutine::new(
            coroutine_function,
            Stack::new(stack_size),
        );

        if let Err(_) = scheduler_handle.work_tx.send(coroutine) {
          return Err(CorosError::TriedToSpawnCoroutineOnShutdownThread)
        }

        Ok(JoinHandle::<T>::new(coroutine_result_rx))
    }

    pub fn spawn<F, T>(&mut self, coroutine_body: F, stack_size: usize) -> Result<JoinHandle<T>>
        where F: FnOnce(IoHandle) -> T + panic::RecoverSafe + Send + 'static,
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
        self.is_running = true;

        let mut thread_pool =  try!(self.thread_pool.write());
        let scheduler_handles = match self.scheduler_handles {
            Some(ref scheduler_handles) => scheduler_handles,
            None => return Err(CorosError::CannotStartPoolWithoutSchedulers),
        };
        thread_pool.scoped(|scoped| {
            for scheduler_handle in scheduler_handles.iter() {
                let mut scheduler = scheduler_handle.scheduler
                    .lock()
                    .expect("Coros internal error: scheduler lock poisoned");
                match scheduler.take() {
                    Some(mut scheduler) => {
                        scoped.execute(move || { scheduler.run() })
                    },
                    None => panic!("Coros internal error: starting pool without full set of schedulers"),
                }
            }
        });
        self.is_running = true;

        Ok(())
    }

    pub fn stop(&mut self) -> Result<()> {
        if !self.is_running {
            return Ok(());
        }
        let mut errors = Vec::with_capacity(self.thread_count as usize);

        {
            let thread_pool =  try!(self.thread_pool.write());
            let scheduler_handles = match self.scheduler_handles {
                Some(ref scheduler_handles) => scheduler_handles,
                None => return Err(CorosError::CannotStartPoolWithoutSchedulers),
            };
            for scheduler_handle in scheduler_handles {
                if let Err(_) = scheduler_handle.shutdown_tx.send(()) {
                    errors.push(CorosError::UnableToSendThreadShutdownSignal);
                }
            }
            thread_pool.join_all();
        }
        {
            let scheduler_result_rx = match self.scheduler_result_rx {
                Some(ref scheduler_result_rx) => scheduler_result_rx,
                None => return Err(CorosError::InvalidPoolNoSchedulerResultReceiver),
            };
            for _ in 0..self.thread_count {
                if let Err(err) = scheduler_result_rx.recv() {
                    errors.push(CorosError::UnableToReceiveThreadShutdownResult(err));
                }
            }
        }
        self.is_running = false;

        try!(self.create_scheduler_handles());

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

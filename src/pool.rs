use std::fmt;
use std::sync::RwLock;
use std::sync::mpsc::channel;
use std::thread;

use rand::{
    Rng,
    thread_rng,
};
use scoped_threadpool::Pool as ThreadPool;

use CoroutineJoinHandle;
use Result;
use thread_scheduler::ThreadScheduler;

pub struct Pool {
    pub name: String,
    thread_pool: RwLock<ThreadPool>,
    thread_schedulers: Vec<ThreadScheduler>
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
        let thread_schedulers = (0..thread_count).map (|_| {
            ThreadScheduler::new()
        }).collect();
        Pool {
           name: name,
           thread_pool: RwLock::new(ThreadPool::new(thread_count)),
           thread_schedulers: thread_schedulers,
        }
    }

    pub fn spawn<F, T>(&self, coroutine_body: F) -> CoroutineJoinHandle<T>
        where F: FnOnce() -> T + Send + 'static,
              T: Send + 'static
    {
        let (coroutine_result_sender, coroutine_result_receiver) = channel();
        let worker_thread = thread_rng()
            .choose(&self.thread_schedulers)
            .expect("Cannot spawn threads on uninitialized pool");

        worker_thread.send(Box::new(move || {
            let maybe_coroutine_result = thread::catch_panic(move || {
                coroutine_body()
            });
            coroutine_result_sender.send(maybe_coroutine_result).unwrap();
        }));

        CoroutineJoinHandle::<T>::new(coroutine_result_receiver)
    }

    pub fn start(&self) -> Result<()> {
        let mut thread_pool =  try!(self.thread_pool.write());
        thread_pool.scoped(|scoped| {
            for thread_scheduler in self.thread_schedulers.iter() {
                scoped.execute(move || {  thread_scheduler.start(); () });
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
        self.stop();
    }
}

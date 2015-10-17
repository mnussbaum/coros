#![feature(drain, fnbox)]

extern crate context;
extern crate deque;
#[macro_use] extern crate log;
extern crate mio;
extern crate rand;
extern crate scoped_threadpool;

mod coroutine;
mod coroutine_handle;
pub use coroutine_handle::CoroutineHandle;
mod coroutine_join_handle;
pub use coroutine_join_handle::CoroutineJoinHandle;
mod error;
pub use error::CoroError;
mod thread_scheduler;
mod pool;
pub use pool::Pool;

use std::result;
pub type Result<'a, T> = result::Result<T, CoroError<'a>>;

#![feature(catch_panic, fnbox)]

extern crate context;
extern crate deque;
extern crate libc;
#[macro_use] extern crate log;
extern crate mio;
extern crate rand;
extern crate scoped_threadpool;

mod coroutine;
mod coroutine_join_handle;
pub use coroutine_join_handle::CoroutineJoinHandle;
mod error;
pub use error::CoroError;
mod thread_scheduler;
mod pool;
pub use pool::Pool;

use std::result;
pub type Result<'a, T> = result::Result<T, CoroError<'a>>;

use std::thread::Result as ThreadResult;
pub type CoroutineResult<T> = ThreadResult<T>;

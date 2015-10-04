#![feature(fnbox)]

extern crate context;
extern crate deque;
extern crate libc;
#[macro_use] extern crate log;
extern crate mio;
extern crate rand;
extern crate scoped_threadpool;

mod coroutine;
pub mod coroutine_join_handle;
pub use coroutine_join_handle::CoroutineJoinHandle;
pub mod error;
pub use error::CoroError;
pub mod thread_scheduler;
pub mod pool;

use std::result;
pub type Result<'a, T> = result::Result<T, CoroError<'a>>;

pub type CoroutineBodyReturn = Send + 'static;
pub type CoroutineBody = FnOnce() -> CoroutineBodyReturn + Send + 'static;

extern crate context;
extern crate deque;
extern crate libc;
#[macro_use] extern crate log;
extern crate mio;
extern crate rand;
extern crate scoped_threadpool;

pub mod coroutine;
pub use coroutine::CoroutineJoinHandle;
pub mod error;
pub use error::CoroError;
pub mod thread_scheduler;
pub mod pool;

use std::result;
pub type Result<'a, T> = result::Result<T, CoroError<'a>>;

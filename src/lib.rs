#![feature(fnbox)]

extern crate context;
extern crate deque;
#[macro_use] extern crate log;
extern crate mio;
extern crate rand;
extern crate scoped_threadpool;

mod coroutine;
pub use coroutine::io_handle::IoHandle;
pub use coroutine::join_handle::JoinHandle;
mod error;
pub use error::CorosError;
pub use coroutine::channel;
mod scheduler;
mod pool;
pub use pool::Pool;

use std::result;
pub type Result<T> = result::Result<T, CorosError>;

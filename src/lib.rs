#![feature(drain, fnbox)]

extern crate context;
extern crate deque;
#[macro_use] extern crate log;
extern crate mio;
extern crate rand;
extern crate scoped_threadpool;

mod coroutine;
mod coroutine_blocking_handle;
pub use coroutine_blocking_handle::CoroutineBlockingHandle;
mod coroutine_join_handle;
pub use coroutine_join_handle::CoroutineJoinHandle;
mod error;
pub use error::CoroError;
mod notifying_channel;
pub use notifying_channel::{
    NotifyingReceiver,
    NotifyingSender,
    notifying_channel,
};
mod thread_scheduler;
mod pool;
pub use pool::Pool;

use std::result;
pub type Result<'a, T> = result::Result<T, CoroError<'a>>;

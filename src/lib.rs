#![feature(fnbox, recover, std_panic)]

extern crate context;
extern crate deque;
#[macro_use] extern crate log;
extern crate mio;
extern crate rand;
extern crate scoped_threadpool;
extern crate slab;

mod coroutine;
pub use coroutine::io_handle::IoHandle;
pub use coroutine::join_handle::JoinHandle;
mod error;
pub use error::CorosError;
pub use coroutine::channel::{
    self,
    Receiver,
    Sender,
};
mod scheduler;
mod pool;
pub use pool::Pool;

use std::result;
pub type Result<T> = result::Result<T, CorosError>;

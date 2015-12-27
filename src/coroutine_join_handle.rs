use std::sync::mpsc::Receiver;

use error::CorosError;
use Result;

pub struct CoroutineJoinHandle<T>
    where T: Send + 'static
{
    coroutine_result_rx: Receiver<T>,
    pub is_joined: bool,
}

impl<T> CoroutineJoinHandle<T>
    where T: Send + 'static
{

    pub fn new(coroutine_result_rx: Receiver<T>) -> CoroutineJoinHandle<T>
    {
        CoroutineJoinHandle {
            coroutine_result_rx: coroutine_result_rx,
            is_joined: false,
        }
    }

    pub fn join(&mut self) -> Result<T> {
        if self.is_joined {
            return Err(CorosError::CoroutineAlreadyJoined)
        }
        self.is_joined = true;

        Ok(try!(self.coroutine_result_rx.recv()))
    }
}

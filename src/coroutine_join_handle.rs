use std::sync::mpsc::Receiver;

use Result;
use CoroutineResult;

pub struct CoroutineJoinHandle<T>
    where T: Send + 'static
{
    coroutine_result_receiver: Receiver<CoroutineResult<T>>,
}

impl<T> CoroutineJoinHandle<T>
    where T: Send + 'static
{

    pub fn new(coroutine_result_receiver: Receiver<CoroutineResult<T>>) -> CoroutineJoinHandle<T>
    {
        CoroutineJoinHandle {
            coroutine_result_receiver: coroutine_result_receiver,
        }
    }

    pub fn join(&self) -> Result<CoroutineResult<T>> {
        let coroutine_result = try!(self.coroutine_result_receiver.recv());
        Ok(coroutine_result)
    }
}

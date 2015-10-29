use std::sync::mpsc::{
    Receiver,
    RecvError,
};

pub struct CoroutineJoinHandle<T>
    where T: Send + 'static
{
    coroutine_result_receiver: Receiver<T>,
    is_joined: bool,
}

impl<T> CoroutineJoinHandle<T>
    where T: Send + 'static
{

    pub fn new(coroutine_result_receiver: Receiver<T>) -> CoroutineJoinHandle<T>
    {
        CoroutineJoinHandle {
            coroutine_result_receiver: coroutine_result_receiver,
            is_joined: false,
        }
    }

    pub fn join(&mut self) -> Result<Option<T>, RecvError> {
        if self.is_joined {
            return Ok(None)
        }

        self.is_joined = true;
        match self.coroutine_result_receiver.recv() {
            Ok(result) => Ok(Some(result)),
            Err(err) => Err(err),
        }
    }
}

impl<T> Drop for CoroutineJoinHandle<T>
    where T: Send + 'static
{
    fn drop(&mut self) {
        self.join().unwrap();
    }
}

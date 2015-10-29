use std::sync::mpsc::{
    Receiver,
    RecvError,
};

pub struct CoroutineJoinHandle<T>
    where T: Send + 'static
{
    coroutine_result_receiver: Receiver<T>,
}

impl<T> CoroutineJoinHandle<T>
    where T: Send + 'static
{

    pub fn new(coroutine_result_receiver: Receiver<T>) -> CoroutineJoinHandle<T>
    {
        CoroutineJoinHandle {
            coroutine_result_receiver: coroutine_result_receiver,
        }
    }

    pub fn join(&self) -> Result<T, RecvError> {
        self.coroutine_result_receiver.recv()
    }
}
//
// impl<T> Drop for CoroutineJoinHandle<T>
//     where T: Send + 'static
// {
//     fn drop(&mut self) {
//         self.join().unwrap();
//     }
// }

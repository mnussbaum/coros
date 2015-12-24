use std::error::{
    Error,
};
use std::fmt;
use std::sync::{
    PoisonError,
    RwLockReadGuard,
    RwLockWriteGuard,
    mpsc,
};

use mio::NotifyError;
use mio::Token;
use scoped_threadpool::Pool as ThreadPool;

#[derive(Debug)]
pub enum CorosError {
    CoroutineChannelSendError,
    InvalidCoroutineNoContext,
    InvalidThreadForSpawn(u32, u32),
    NotifyError(NotifyError<Token>),
    RecvError(mpsc::RecvError),
    ThreadPoolReadLockPoisoned,
    ThreadPoolWriteLockPoisoned,
}

impl CorosError {
    pub fn description(&self) -> &str {
        match *self {
            CorosError::CoroutineChannelSendError => {
                "Cannot send message via channel to a finshed coroutine"
            },
            CorosError::InvalidCoroutineNoContext => {
                "Coroutine attempting to run in invalid state, has no execution context"
            },
            CorosError::InvalidThreadForSpawn(_, _) => {
                "Index of thread for coroutine spawn greater then thread count"
            },
            CorosError::NotifyError(ref err) => err.description(),
            CorosError::RecvError(ref err) => err.description(),
            CorosError::ThreadPoolReadLockPoisoned => {
                "Pool's thread pool read lock poisoned"
            },
            CorosError::ThreadPoolWriteLockPoisoned => {
                "Pool's thread pool write lock poisoned"
            },
        }
    }
}

impl Error for CorosError {
    fn description(&self) -> &str {
        self.description()
    }

    fn cause(&self) -> Option<&Error> {
        match *self {
            CorosError::CoroutineChannelSendError => None,
            CorosError::InvalidCoroutineNoContext => None,
            CorosError::InvalidThreadForSpawn(_, _) => None,
            CorosError::NotifyError(ref err) => Some(err),
            CorosError::RecvError(ref err) => Some(err),
            CorosError::ThreadPoolReadLockPoisoned => None,
            CorosError::ThreadPoolWriteLockPoisoned => None,
        }
    }
}

impl fmt::Display for CorosError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}", self.description())
    }
}

impl From<NotifyError<Token>> for CorosError {
    fn from(err: NotifyError<Token>) -> CorosError {
        error!("Error notifying coroutine");
        CorosError::NotifyError(err)
    }
}

impl From<mpsc::RecvError> for CorosError {
    fn from(err: mpsc::RecvError) -> CorosError {
        CorosError::RecvError(err)
    }
}

impl<'a> From<PoisonError<RwLockReadGuard<'a, ThreadPool>>> for CorosError {
    fn from(err: PoisonError<RwLockReadGuard<'a, ThreadPool>>) -> CorosError {
        error!("Error obtaining thread pool read lock {:?}", err);
        CorosError::ThreadPoolReadLockPoisoned
    }
}

impl<'a> From<PoisonError<RwLockWriteGuard<'a, ThreadPool>>> for CorosError {
    fn from(err: PoisonError<RwLockWriteGuard<'a, ThreadPool>>) -> CorosError {
        error!("Error obtaining thread pool write lock {:?}", err);
        CorosError::ThreadPoolWriteLockPoisoned
    }
}

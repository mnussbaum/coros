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
pub enum CorosError<'a> {
    CoroutineChannelSendError,
    InvalidThreadForSpawn(u32, u32),
    NotifyError(NotifyError<Token>),
    RecvError(mpsc::RecvError),
    ThreadPoolReadLockPoisoned(PoisonError<RwLockReadGuard<'a, ThreadPool>>),
    ThreadPoolWriteLockPoisoned(PoisonError<RwLockWriteGuard<'a, ThreadPool>>),
}

impl<'a> CorosError<'a> {
    pub fn description(&self) -> &str {
        match *self {
            CorosError::CoroutineChannelSendError => {
                "Cannot send message via channel to a finshed coroutine"
            },
            CorosError::InvalidThreadForSpawn(_, _) => {
                "Index of thread for coroutine spawn greater then thread count"
            },
            CorosError::NotifyError(ref err) => err.description(),
            CorosError::RecvError(ref err) => err.description(),
            CorosError::ThreadPoolReadLockPoisoned(_) => {
                "Pool's thread pool read lock poisoned"
            },
            CorosError::ThreadPoolWriteLockPoisoned(_) => {
                "Pool's thread pool write lock poisoned"
            },
        }
    }
}

impl<'a> Error for CorosError<'a> {
    fn description(&self) -> &str {
        self.description()
    }

    fn cause(&self) -> Option<&Error> {
        match *self {
            CorosError::CoroutineChannelSendError => None,
            CorosError::InvalidThreadForSpawn(_, _) => None,
            CorosError::NotifyError(ref err) => Some(err),
            CorosError::RecvError(ref err) => Some(err),
            CorosError::ThreadPoolReadLockPoisoned(_) => None,
            CorosError::ThreadPoolWriteLockPoisoned(_) => None,
        }
    }
}

impl<'a> fmt::Display for CorosError<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}", self.description())
    }
}

impl<'a> From<NotifyError<Token>> for CorosError<'a> {
    fn from(err: NotifyError<Token>) -> CorosError<'a> {
        error!("Error notifying coroutine");
        CorosError::NotifyError(err)
    }
}

impl<'a> From<mpsc::RecvError> for CorosError<'a> {
    fn from(err: mpsc::RecvError) -> CorosError<'a> {
        CorosError::RecvError(err)
    }
}

impl<'a> From<PoisonError<RwLockReadGuard<'a, ThreadPool>>> for CorosError<'a> {
    fn from(err: PoisonError<RwLockReadGuard<'a, ThreadPool>>) -> CorosError<'a> {
        error!("Error obtaining thread pool read lock {:?}", err);
        CorosError::ThreadPoolReadLockPoisoned(err)
    }
}

impl<'a> From<PoisonError<RwLockWriteGuard<'a, ThreadPool>>> for CorosError<'a> {
    fn from(err: PoisonError<RwLockWriteGuard<'a, ThreadPool>>) -> CorosError<'a> {
        error!("Error obtaining thread pool write lock {:?}", err);
        CorosError::ThreadPoolWriteLockPoisoned(err)
    }
}

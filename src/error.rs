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

use scoped_threadpool::Pool as ThreadPool;

#[derive(Debug)]
pub enum CoroError<'a> {
    RecvError(mpsc::RecvError),
    ThreadPoolReadLockPoisoned(PoisonError<RwLockReadGuard<'a, ThreadPool>>),
    ThreadPoolWriteLockPoisoned(PoisonError<RwLockWriteGuard<'a, ThreadPool>>),
}

impl<'a> CoroError<'a> {
    pub fn description(&self) -> &str {
        match *self {
            CoroError::RecvError(ref err) => err.description(),
            CoroError::ThreadPoolReadLockPoisoned(_) => {
                "Pool's thread pool read lock poisoned"
            },
            CoroError::ThreadPoolWriteLockPoisoned(_) => {
                "Pool's thread pool write lock poisoned"
            },
        }
    }
}

impl<'a> Error for CoroError<'a> {
    fn description(&self) -> &str {
        self.description()
    }

    fn cause(&self) -> Option<&Error> {
        match *self {
            CoroError::RecvError(ref err) => Some(err),
            CoroError::ThreadPoolReadLockPoisoned(_) => None,
            CoroError::ThreadPoolWriteLockPoisoned(_) => None,
        }
    }
}

impl<'a> fmt::Display for CoroError<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}", self.description())
    }
}

impl<'a> From<mpsc::RecvError> for CoroError<'a> {
    fn from(err: mpsc::RecvError) -> CoroError<'a> {
        CoroError::RecvError(err)
    }
}

impl<'a> From<PoisonError<RwLockReadGuard<'a, ThreadPool>>> for CoroError<'a> {
    fn from(err: PoisonError<RwLockReadGuard<'a, ThreadPool>>) -> CoroError<'a> {
        error!("Error obtaining thread pool read lock {:?}", err);
        CoroError::ThreadPoolReadLockPoisoned(err)
    }
}

impl<'a> From<PoisonError<RwLockWriteGuard<'a, ThreadPool>>> for CoroError<'a> {
    fn from(err: PoisonError<RwLockWriteGuard<'a, ThreadPool>>) -> CoroError<'a> {
        error!("Error obtaining thread pool write lock {:?}", err);
        CoroError::ThreadPoolWriteLockPoisoned(err)
    }
}

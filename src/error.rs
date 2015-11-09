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
pub enum CorosError<'a> {
    RecvError(mpsc::RecvError),
    ThreadPoolReadLockPoisoned(PoisonError<RwLockReadGuard<'a, ThreadPool>>),
    ThreadPoolWriteLockPoisoned(PoisonError<RwLockWriteGuard<'a, ThreadPool>>),
}

impl<'a> CorosError<'a> {
    pub fn description(&self) -> &str {
        match *self {
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

use std::collections::hash_map::HashMap;
use std::error::{
    Error,
};
use std::fmt;
use std::io::Error as IoError;
use std::sync::{
    MutexGuard,
    PoisonError,
    RwLockReadGuard,
    RwLockWriteGuard,
    mpsc,
};

use context::error::ContextError;
use deque::Stealer;
use mio::{
    NotifyError,
    Token,
    TimerError,
};
use scoped_threadpool::Pool as ThreadPool;

use coroutine::Coroutine;

#[derive(Debug)]
pub enum CorosError {
    CannotStartPoolWithoutSchedulers,
    CoroutineBlockedOnIoAwokenForNotIo,
    CoroutineBlockSendError,
    CoroutineChannelSendError,
    InvalidCoroutineContext(ContextError),
    InvalidCoroutineNoCallback,
    InvalidCoroutineNoContext,
    InvalidCoroutineSlabContents,
    InvalidThreadForSpawn(u32, u32),
    MioIoError(IoError),
    MioTimerError(TimerError),
    MioNotifyError(NotifyError<Token>),
    MissingCoroutine,
    RecvError(mpsc::RecvError),
    SendIoResultToCoroutineError,
    SlabFullError,
    ThreadPoolReadLockPoisoned,
    ThreadPoolWriteLockPoisoned,
    TriedToSpawnCoroutineOnShutdownThread,
    TryRecvError(mpsc::TryRecvError),
    UnableToReceiveThreadShutdownResult(mpsc::RecvError),
    UnableToSendThreadShutdownSignal,
    UncleanShutdown(HashMap<u32, CorosError>),
    WorkStealerMutexPoisoned,
}

impl CorosError {
    pub fn description(&self) -> &str {
        match *self {
            CorosError::CannotStartPoolWithoutSchedulers => {
                "Cannot start pool without schedulers"
            },
            CorosError::CoroutineBlockedOnIoAwokenForNotIo => {
                "Coroutine was blocked on IO but awoken for not IO"
            }
            CorosError::CoroutineBlockSendError => {
                "Cannot send message to block coroutine"
            },
            CorosError::CoroutineChannelSendError => {
                "Cannot send message via channel to a finshed coroutine"
            },
            CorosError::InvalidCoroutineContext(ref err) => err.description(),
            CorosError::InvalidCoroutineNoCallback => {
                "Coroutine in invalid state, has no execution callback"
            },
            CorosError::InvalidCoroutineNoContext => {
                "Coroutine attempting to run in invalid state, has no execution context"
            },
            CorosError::InvalidCoroutineSlabContents => {
                "Invalid coroutine slab contents"
            }
            CorosError::InvalidThreadForSpawn(_, _) => {
                "Index of thread for coroutine spawn greater then thread count"
            },
            CorosError::MioIoError(ref err) => err.description(),
            CorosError::MioTimerError(ref err) => err.description(),
            CorosError::MioNotifyError(ref err) => err.description(),
            CorosError::MissingCoroutine => {
                "Attempting to fetch missing coroutine from suspension"
            }
            CorosError::RecvError(ref err) => err.description(),
            CorosError::SendIoResultToCoroutineError => {
                "Error sending IO result to coroutine"
            },
            CorosError::SlabFullError => {
                "Error attempting to insert a suspended coroutine into a full slab"
            }
            CorosError::ThreadPoolReadLockPoisoned => {
                "Pool's thread pool read lock poisoned"
            },
            CorosError::ThreadPoolWriteLockPoisoned => {
                "Pool's thread pool write lock poisoned"
            },
            CorosError::TriedToSpawnCoroutineOnShutdownThread => {
                "Pool tried to spawn coroutine onto a native thread that is shutdown"
            },
            CorosError::TryRecvError(ref err) => err.description(),
            CorosError::UnableToReceiveThreadShutdownResult(ref err) => err.description(),
            CorosError::UnableToSendThreadShutdownSignal => {
                "Error sending shutdown message to native thread"
            },
            CorosError::UncleanShutdown(_) => {
                "Unable to shutdown all native threads"
            }
            CorosError::WorkStealerMutexPoisoned => {
                "Thread scheduler work stealer mutex poisoned"
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
            CorosError::CannotStartPoolWithoutSchedulers => None,
            CorosError::CoroutineBlockedOnIoAwokenForNotIo => None,
            CorosError::CoroutineBlockSendError => None,
            CorosError::CoroutineChannelSendError => None,
            CorosError::InvalidCoroutineContext(ref err) => Some(err),
            CorosError::InvalidCoroutineNoCallback => None,
            CorosError::InvalidCoroutineNoContext => None,
            CorosError::InvalidCoroutineSlabContents => None,
            CorosError::InvalidThreadForSpawn(_, _) => None,
            CorosError::MioIoError(ref err) => Some(err),
            CorosError::MioTimerError(ref err) => Some(err),
            CorosError::MioNotifyError(ref err) => Some(err),
            CorosError::MissingCoroutine => None,
            CorosError::RecvError(ref err) => Some(err),
            CorosError::SendIoResultToCoroutineError => None,
            CorosError::SlabFullError => None,
            CorosError::ThreadPoolReadLockPoisoned => None,
            CorosError::ThreadPoolWriteLockPoisoned => None,
            CorosError::TriedToSpawnCoroutineOnShutdownThread => None,
            CorosError::TryRecvError(ref err) => Some(err),
            CorosError::UnableToReceiveThreadShutdownResult(ref err) => Some(err),
            CorosError::UnableToSendThreadShutdownSignal => None,
            CorosError::UncleanShutdown(_) => None,
            CorosError::WorkStealerMutexPoisoned => None,
        }
    }
}

impl fmt::Display for CorosError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}", self.description())
    }
}

impl From<ContextError> for CorosError {
    fn from(err: ContextError) -> CorosError {
        CorosError::InvalidCoroutineContext(err)
    }
}

impl From<IoError> for CorosError {
    fn from(err: IoError) -> CorosError {
        CorosError::MioIoError(err)
    }
}

impl From<TimerError> for CorosError {
    fn from(err: TimerError) -> CorosError {
        CorosError::MioTimerError(err)
    }
}

impl From<NotifyError<Token>> for CorosError {
    fn from(err: NotifyError<Token>) -> CorosError {
        error!("Error notifying coroutine");
        CorosError::MioNotifyError(err)
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

impl<'a> From<PoisonError<MutexGuard<'a, Vec<Stealer<Coroutine>>>>> for CorosError {
    fn from(err: PoisonError<MutexGuard<'a, Vec<Stealer<Coroutine>>>>) -> CorosError {
        error!("Error obtaining thread scheduler work stealer lock {:?}", err);
        CorosError::WorkStealerMutexPoisoned
    }
}

impl From<mpsc::RecvError> for CorosError {
    fn from(err: mpsc::RecvError) -> CorosError {
        CorosError::RecvError(err)
    }
}

impl From<mpsc::TryRecvError> for CorosError {
    fn from(err: mpsc::TryRecvError) -> CorosError {
        CorosError::TryRecvError(err)
    }
}

use std::sync::mpsc::{
    channel,
    Receiver as StdReceiver,
    RecvError,
    Sender as StdSender,
};

use mio::Token;
use mio::Sender as MioSender;

use error::CorosError;
use Result as CorosResult;

pub struct BlockedMessage {
    pub mio_tx: MioSender<Token>,
    pub token: Token,
}

pub struct Sender<M: Send> {
    blocked_message_rx: StdReceiver<BlockedMessage>,
    user_message_tx: StdSender<M>,
}

impl<M: Send> Sender<M> {
    // Is there a way to implement this without blocking while allowing messages to send before
    // recv is called?
    pub fn send(&self, message: M) -> CorosResult<()> {
        let blocked_message = try!(self.blocked_message_rx.recv());
        try!(
            blocked_message
             .mio_tx
             .send(blocked_message.token)
        );

        if let Err(_) = self.user_message_tx.send(message) {
            return Err(CorosError::CoroutineChannelSendError)
        }

        Ok(())
    }
}

pub struct Receiver<M: Send> {
    pub blocked_message_tx: StdSender<BlockedMessage>,
    user_message_rx: StdReceiver<M>,
}

impl<M: Send> Receiver<M> {
    pub fn recv(&self) -> Result<M, RecvError> {
        self.user_message_rx.recv()
    }
}

pub fn new<M: Send>() -> (Sender<M>, Receiver<M>) {
    let (blocked_message_tx, blocked_message_rx) = channel::<BlockedMessage>();
    let (user_message_tx, user_message_rx) = channel::<M>();
    let tx = Sender::<M> {
      blocked_message_rx: blocked_message_rx,
      user_message_tx: user_message_tx,
    };
    let rx = Receiver::<M> {
      blocked_message_tx: blocked_message_tx,
      user_message_rx: user_message_rx,
    };

    (tx, rx)
}

use std::sync::mpsc::{
    channel,
    Receiver,
    RecvError,
    Sender,
};

use mio::Token;
use mio::Sender as MioSender;

use error::CorosError;
use Result as CorosResult;

pub struct BlockedMessage {
    pub mio_tx: MioSender<Token>,
    pub token: Token,
}

pub struct CoroutineSender<M: Send> {
    blocked_message_rx: Receiver<BlockedMessage>,
    user_message_tx: Sender<M>,
}

impl<M: Send> CoroutineSender<M> {
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

pub struct CoroutineReceiver<M: Send> {
    pub blocked_message_tx: Sender<BlockedMessage>,
    user_message_rx: Receiver<M>,
}

impl<M: Send> CoroutineReceiver<M> {
    pub fn recv(&self) -> Result<M, RecvError> {
        self.user_message_rx.recv()
    }
}

pub fn coroutine_channel<M: Send>() -> (CoroutineSender<M>, CoroutineReceiver<M>) {
    let (blocked_message_tx, blocked_message_rx) = channel::<BlockedMessage>();
    let (user_message_tx, user_message_rx) = channel::<M>();
    let tx = CoroutineSender::<M> {
      blocked_message_rx: blocked_message_rx,
      user_message_tx: user_message_tx,
    };
    let rx = CoroutineReceiver::<M> {
      blocked_message_tx: blocked_message_tx,
      user_message_rx: user_message_rx,
    };

    (tx, rx)
}

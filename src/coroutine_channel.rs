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
    pub mio_sender: MioSender<Token>,
    pub token: Token,
}

pub struct CoroutineSender<M: Send> {
    blocked_message_receiver: Receiver<BlockedMessage>,
    user_message_sender: Sender<M>,
}

impl<M: Send> CoroutineSender<M> {
    // Is there a way to implement this without blocking while allowing messages to send before
    // recv is called?
    pub fn send(&self, message: M) -> CorosResult<()> {
        let blocked_message = try!(self.blocked_message_receiver.recv());
        try!(
            blocked_message
             .mio_sender
             .send(blocked_message.token)
        );

        if let Err(_) = self.user_message_sender.send(message) {
            return Err(CorosError::CoroutineChannelSendError)
        }

        Ok(())
    }
}

pub struct CoroutineReceiver<M: Send> {
    pub blocked_message_sender: Sender<BlockedMessage>,
    user_message_receiver: Receiver<M>,
}

impl<M: Send> CoroutineReceiver<M> {
    pub fn recv(&self) -> Result<M, RecvError> {
        self.user_message_receiver.recv()
    }
}

pub fn coroutine_channel<M: Send>() -> (CoroutineSender<M>, CoroutineReceiver<M>) {
    let (blocked_message_sender, blocked_message_receiver) = channel::<BlockedMessage>();
    let (user_message_sender, user_message_receiver) = channel::<M>();
    let sender = CoroutineSender::<M> {
      blocked_message_receiver: blocked_message_receiver,
      user_message_sender: user_message_sender,
    };
    let receiver = CoroutineReceiver::<M> {
      blocked_message_sender: blocked_message_sender,
      user_message_receiver: user_message_receiver,
    };

    (sender, receiver)
}

use std::sync::mpsc::{
    channel,
    Receiver,
    RecvError,
    Sender,
};

use mio::Token;
use mio::Sender as MioSender;

pub struct BlockedMessage {
    pub mio_sender: MioSender<Token>,
    pub token: Token,
}

pub struct NotifyingSender<M: Send> {
    blocked_message_receiver: Receiver<BlockedMessage>,
    user_message_sender: Sender<M>,
}

impl<M: Send> NotifyingSender<M> {
    // Is there a way to implement this without blocking while allowing messages to send before
    // notify_sender_of_blocking is called?
    pub fn send(&self, message: M) {
        match self.blocked_message_receiver.recv() {
            Ok(blocked_message) => {
                blocked_message
                    .mio_sender
                    .send(blocked_message.token)
                    .unwrap(); //TODO: handle errors
                self.user_message_sender.send(message).unwrap() // TODO: handle errors
            },
            Err(e) => panic!("Coros internal error: error setting up channel, {}", e),
        }
    }
}

pub struct NotifyingReceiver<M: Send> {
    pub blocked_message_sender: Sender<BlockedMessage>,
    user_message_receiver: Receiver<M>,
}

impl<M: Send> NotifyingReceiver<M> {
    pub fn notify_sender_of_blocking(&self, mio_sender: MioSender<Token>, token: Token) {
        let message = BlockedMessage {
            mio_sender: mio_sender,
            token: token,
        };
        self.blocked_message_sender.send(message).unwrap(); //TODO: handle errors
    }

    pub fn recv(&self) -> Result<M, RecvError> {
        match self.user_message_receiver.recv() {
            Ok(message) => Ok(message),
            Err(err) => Err(err),
        }
    }
}

pub fn notifying_channel<M: Send>() -> (NotifyingSender<M>, NotifyingReceiver<M>) {
    let (blocked_message_sender, blocked_message_receiver) = channel::<BlockedMessage>();
    let (user_message_sender, user_message_receiver) = channel::<M>();
    let sender = NotifyingSender::<M> {
      blocked_message_receiver: blocked_message_receiver,
      user_message_sender: user_message_sender,
    };
    let receiver = NotifyingReceiver::<M> {
      blocked_message_sender: blocked_message_sender,
      user_message_receiver: user_message_receiver,
    };

    (sender, receiver)
}

use std::{thread::JoinHandle, sync::RwLock};

use flume::{Sender, Receiver};

use crate::prelude::*;

/// Light wrapper around a channel pair and event loop combo. 
pub struct Consumer<T: Send + 'static> {
    /// Handle for the receiving thread.
    handle: JoinHandle<Result<()>>,
    /// The sending end of the channel.
    /// A [`RwLock`] is used so that flushing operations can put a hold on sends.
    sink: RwLock<Sender<T>>,
}

impl<T: Send + 'static> Consumer<T> {
    /// Creates a new consumer using a simple event loop.
    /// 
    /// The provided closure is invoked with each message received by the consumer; 
    /// handling the channel itself is done externally.
    pub fn new(mut handler: impl FnMut(T) -> Result<()> + Send + 'static) -> Self {
        Self::new_manual(move |stream: Receiver<T>| {
            while let Ok(msg) = stream.recv() {
                handler(msg)?;
            }
            Ok(())
        })
    }

    /// Creates a new consumer using a caller-defined event loop.
    /// 
    /// The provided closure is given the receiving end of the channel, and is responsible for its proper handling.
    /// Useful if the event loop needs to set up some non-`Send`/`'static` state.
    pub fn new_manual(mut handler: impl FnMut(Receiver<T>) -> Result<()> + Send + 'static) -> Self {
        let (sink, stream) = flume::unbounded::<T>();

        let handle = std::thread::spawn(move || -> Result<()> {
            if let Err(e) = handler(stream) {
                error!("Consumer terminated due to handler error: {e}");
                Err(e)
            } else {
                Ok(())
            }
        });

        Consumer {
            handle,
            sink: RwLock::new(sink)
        }
    }

    /// Send a message into the consumer's channel.
    /// This method never blocks *unless* the consumer is currently being flushed.
    pub fn send(&self, msg: T) {
        self.sink
            .read()
            .unwrap()
            .send(msg)
            .expect("Consumer channel should not be closed.")
    }

    /// Non-destructively flushes the consumer.
    /// This method locks the sending end of the channel and blocks until it is empty or disconnected.
    pub fn flush(&self) {
        let sink = self.sink.write().unwrap();
        while !sink.is_empty() && !sink.is_disconnected() {
            std::thread::yield_now();
        }
    }

    /// Finalizes the consumer.
    /// This method flushes the internal channel, then terminates/joins the receiving thread.
    pub fn finalize(self) -> Result<()> {
        self.flush();
        drop(self.sink);

        self.handle
            .join()
            .expect("Consumer receiving thread panicked.")
    }
}


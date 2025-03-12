pub mod asynch;
pub mod thread;

pub struct Sender<T> {
    sender: flume::Sender<T>,
}
impl<T> Sender<T> {
    pub fn send_sync(&self, msg: T) {
        self.sender.send(msg).unwrap();
    }

    pub async fn send(&self, msg: T) {
        self.sender.send_async(msg).await.unwrap();
    }
}
impl<T> Clone for Sender<T> {
    fn clone(&self) -> Self {
        let sender = self.sender.clone();
        Self { sender }
    }
}

pub struct Receiver<T> {
    receiver: flume::Receiver<T>,
}
impl<T> Receiver<T> {
    pub fn recv_sync(&self) -> Option<T> {
        match self.receiver.recv() {
            Ok(e) => Some(e),
            Err(_) => None,
        }
    }

    pub async fn recv(&self) -> Option<T> {
        match self.receiver.recv_async().await {
            Ok(e) => Some(e),
            Err(_) => None,
        }
    }
}
impl<T> Clone for Receiver<T> {
    fn clone(&self) -> Self {
        let receiver = self.receiver.clone();
        Self { receiver }
    }
}

pub fn channel<T>(cap: usize) -> (Sender<T>, Receiver<T>) {
    let (sender, receiver) = flume::bounded::<T>(cap);
    (Sender { sender }, Receiver { receiver })
}

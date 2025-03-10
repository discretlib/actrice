use std::sync::{Arc, Mutex};

use crate::Receiver;
use crate::Sender;
use crate::channel;
use async_trait::async_trait;
use tokio::sync::Notify;

pub trait Actor: Sized + Send + 'static {
    fn start(self) -> Context<Self> {
        Context::new(self)
    }
}

pub trait AsyncHandler<M, R>
where
    R: Send + 'static,
{
    fn handle(&mut self, msg: M) -> impl std::future::Future<Output = R> + Send;
}

#[async_trait]
pub trait AsyncChannelHandler<M, R> {
    async fn handle(&mut self, msg: M, sender: &Sender<R>);
}

#[derive(Clone)]
pub struct Context<A>
where
    A: Actor + Send + 'static,
{
    sender: Sender<Enveloppe<A>>,
}
impl<A: Actor + Send + 'static> Context<A> {
    fn new(mut act: A) -> Context<A> {
        let (sender, receiv) = channel::<Enveloppe<A>>(1);

        tokio::spawn(async move {
            while let Ok(fun) = receiv.recv_async().await {
                let mut message = fun.0;
                message.handle(&mut act).await;
            }
        });

        Context { sender }
    }

    pub async fn send<M, R>(&self, msg: M) -> R
    where
        M: Send + 'static,
        R: Send + 'static,
        A: AsyncHandler<M, R>,
    {
        let send_result: Arc<SendResult<R>> = SendResult::new();
        let proxy: AsyncEnveloppeProxy<M, R> = AsyncEnveloppeProxy {
            msg: Some(msg),
            result: send_result.clone(),
        };

        self.sender
            .send_async(Enveloppe(Box::new(proxy)))
            .await
            .unwrap();
        send_result.notify.notified().await;
        send_result.value.lock().unwrap().take().unwrap()
    }

    pub async fn send_with_channel<M, R>(&self, msg: M) -> Receiver<R>
    where
        M: Send + 'static,
        R: Send + 'static,
        A: AsyncChannelHandler<M, R>,
    {
        let (send, receiv) = channel(10);
        let proxy: AsyncChannelEnveloppeProxy<M, R> = AsyncChannelEnveloppeProxy {
            msg: Some(msg),
            sender: send,
        };
        self.sender
            .send_async(Enveloppe(Box::new(proxy)))
            .await
            .unwrap();
        receiv
    }

    pub async fn send_with_provided_channel<M, R>(&self, msg: M, sender: Sender<R>)
    where
        M: Send + 'static,
        R: Send + 'static,
        A: AsyncChannelHandler<M, R>,
    {
        let proxy: AsyncChannelEnveloppeProxy<M, R> = AsyncChannelEnveloppeProxy {
            msg: Some(msg),
            sender,
        };
        self.sender
            .send_async(Enveloppe(Box::new(proxy)))
            .await
            .unwrap();
    }
}

struct Enveloppe<A>(Box<dyn EnveloppeProxy<A> + Send>);

#[async_trait]
trait EnveloppeProxy<A> {
    async fn handle(&mut self, act: &mut A);
}

struct SendResult<R> {
    value: Mutex<Option<R>>,
    notify: Notify,
}
impl<R> SendResult<R> {
    fn new() -> Arc<SendResult<R>> {
        let send: SendResult<R> = Self {
            value: Mutex::new(None),
            notify: Notify::new(),
        };
        Arc::new(send)
    }
}

struct AsyncEnveloppeProxy<M, R>
where
    M: Send + 'static,
    R: Send + 'static,
{
    msg: Option<M>,
    result: Arc<SendResult<R>>,
}
#[async_trait]
impl<A, M, R> EnveloppeProxy<A> for AsyncEnveloppeProxy<M, R>
where
    M: Send + 'static,
    R: Send + 'static,
    A: Actor + AsyncHandler<M, R> + Send + 'static,
{
    async fn handle(&mut self, act: &mut A) {
        let result = act.handle(self.msg.take().unwrap()).await;
        let mut v = self.result.value.lock().unwrap();
        *v = Some(result);
        self.result.notify.notify_one();
    }
}

struct AsyncChannelEnveloppeProxy<M, R>
where
    M: Send + 'static,
    R: Send + 'static,
{
    msg: Option<M>,
    sender: Sender<R>,
}
#[async_trait]
impl<A, M, R> EnveloppeProxy<A> for AsyncChannelEnveloppeProxy<M, R>
where
    M: Send + 'static,
    R: Send + 'static,
    A: Actor + AsyncChannelHandler<M, R> + Send + 'static,
{
    async fn handle(&mut self, act: &mut A) {
        act.handle(self.msg.take().unwrap(), &self.sender).await;
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[tokio::test]
    async fn send() {
        struct Greeter;
        impl Actor for Greeter {}

        impl AsyncHandler<String, String> for Greeter {
            async fn handle(&mut self, msg: String) -> String {
                format!("Hello {msg}")
            }
        }

        let ctx = Greeter.start();
        let res: String = ctx.send("World".to_owned()).await;

        assert_eq!("Hello World", res)
    }

    #[tokio::test]
    async fn send_channel() {
        struct Repeat {
            value: String,
            number: usize,
        }

        struct Repeater;
        impl Actor for Repeater {}

        #[async_trait]
        impl AsyncChannelHandler<Repeat, String> for Repeater {
            async fn handle(&mut self, msg: Repeat, sender: &Sender<String>) {
                for _ in 0..msg.number {
                    sender.send_async(msg.value.to_owned()).await.unwrap();
                }
            }
        }

        let ctx = Repeater.start();
        let number = 10;

        let chan = ctx
            .send_with_channel(Repeat {
                value: "Hello World".to_owned(),
                number: number,
            })
            .await;
        let mut expected = 0;
        while let Ok(_) = chan.recv_async().await {
            expected += 1;
        }
        assert_eq!(number, expected);

        let (send, receiv) = channel(10);
        ctx.send_with_provided_channel(
            Repeat {
                value: "Hello World".to_owned(),
                number: number,
            },
            send,
        )
        .await;
        let mut expected = 0;
        while let Ok(_) = receiv.recv_async().await {
            expected += 1;
        }
        assert_eq!(number, expected);
    }
}

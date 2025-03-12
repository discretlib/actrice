use crate::Receiver;
use crate::Sender;
use crate::channel;
use async_trait::async_trait;
use tokio::sync::mpsc;
use tokio::sync::oneshot;

pub trait Actor: Sized + Send + 'static {
    fn start(self) -> Adress<Self> {
        Adress::new(self)
    }
}

pub trait AsyncHandler<M, R>
where
    R: Send + 'static,
{
    fn handle(&mut self, msg: M) -> impl std::future::Future<Output = R> + Send;
}

pub trait AsyncChannelHandler<M, R> {
    fn handle(
        &mut self,
        msg: M,
        sender: &Sender<R>,
    ) -> impl std::future::Future<Output = ()> + Send;
}

#[derive(Clone)]
pub struct Adress<A>
where
    A: Actor + Send + 'static,
{
    sender: mpsc::Sender<Enveloppe<A>>,
}
impl<A: Actor + Send + 'static> Adress<A> {
    fn new(mut act: A) -> Adress<A> {
        let (sender, mut receiv) = mpsc::channel::<Enveloppe<A>>(1);

        tokio::spawn(async move {
            while let Some(fun) = receiv.recv().await {
                let mut message = fun.0;
                message.handle(&mut act).await;
            }
        });

        Adress { sender }
    }

    pub async fn send<M, R>(&self, msg: M)
    where
        M: Send + 'static,
        R: Send + 'static,
        A: AsyncHandler<M, R>,
    {
        // let send_result: Arc<SendResult<R>> = SendResult::new();
        let proxy: AsyncEnveloppeProxy<M, R> = AsyncEnveloppeProxy {
            msg: Some(msg),
            sender: None, // send_result.clone(),
        };

        self.sender.send(Enveloppe(Box::new(proxy))).await.unwrap();
    }

    pub async fn ask<M, R>(&self, msg: M) -> R
    where
        M: Send + 'static,
        R: Send + 'static,
        A: AsyncHandler<M, R>,
    {
        let (tx, rx) = oneshot::channel();
        let proxy: AsyncEnveloppeProxy<M, R> = AsyncEnveloppeProxy {
            msg: Some(msg),
            sender: Some(tx), // send_result.clone(),
        };

        self.sender.send(Enveloppe(Box::new(proxy))).await.unwrap();
        rx.await.unwrap()
    }

    pub async fn ask_channel<M, R>(&self, msg: M) -> Receiver<R>
    where
        M: Send + 'static,
        R: Send + 'static,
        A: AsyncChannelHandler<M, R>,
    {
        let (send, receiv) = channel(1);
        let proxy: AsyncChannelEnveloppeProxy<M, R> = AsyncChannelEnveloppeProxy {
            msg: Some(msg),
            sender: send,
        };
        self.sender.send(Enveloppe(Box::new(proxy))).await.unwrap();
        receiv
    }

    pub async fn ask_with<M, R>(&self, msg: M, sender: Sender<R>)
    where
        M: Send + 'static,
        R: Send + 'static,
        A: AsyncChannelHandler<M, R>,
    {
        let proxy: AsyncChannelEnveloppeProxy<M, R> = AsyncChannelEnveloppeProxy {
            msg: Some(msg),
            sender,
        };
        self.sender.send(Enveloppe(Box::new(proxy))).await.unwrap();
    }
}

struct Enveloppe<A>(Box<dyn EnveloppeProxy<A> + Send>);

#[async_trait]
trait EnveloppeProxy<A> {
    async fn handle(&mut self, act: &mut A);
}

struct AsyncEnveloppeProxy<M, R>
where
    M: Send + 'static,
    R: Send + 'static,
{
    msg: Option<M>,
    sender: Option<oneshot::Sender<R>>,
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
        if let Some(sender) = self.sender.take() {
            let _ = sender.send(result);
        }
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
    async fn ask() {
        struct Greeter;
        impl Actor for Greeter {}

        impl AsyncHandler<String, String> for Greeter {
            async fn handle(&mut self, msg: String) -> String {
                format!("Hello {msg}")
            }
        }

        let ctx = Greeter.start();
        let res: String = ctx.ask("World".to_owned()).await;

        assert_eq!("Hello World", res)
    }

    #[tokio::test]
    async fn ask_channel() {
        struct Repeat {
            value: String,
            number: usize,
        }

        struct Repeater;
        impl Actor for Repeater {}

        impl AsyncChannelHandler<Repeat, String> for Repeater {
            async fn handle(&mut self, msg: Repeat, sender: &Sender<String>) {
                for _ in 0..msg.number {
                    sender.send(msg.value.to_owned()).await;
                }
            }
        }

        let ctx = Repeater.start();
        let number = 10;

        let chan = ctx
            .ask_channel(Repeat {
                value: "Hello World".to_owned(),
                number: number,
            })
            .await;
        let mut expected = 0;
        while let Some(_) = chan.recv().await {
            expected += 1;
        }
        assert_eq!(number, expected);

        let (send, receiv) = channel(10);
        ctx.ask_with(
            Repeat {
                value: "Hello World".to_owned(),
                number: number,
            },
            send,
        )
        .await;
        let mut expected = 0;
        while let Some(_) = receiv.recv().await {
            expected += 1;
        }
        assert_eq!(number, expected);
    }

    #[tokio::test]
    async fn send() {
        struct Greeter {
            sender: Sender<String>,
        }
        impl Actor for Greeter {}

        impl AsyncHandler<String, ()> for Greeter {
            async fn handle(&mut self, msg: String) -> () {
                self.sender.send(format!("Hello {msg}")).await
            }
        }

        let (sender, receiver) = channel(1);
        let greeter = Greeter { sender };
        let ctx = greeter.start();
        ctx.send("World".to_owned()).await;
        if let Some(res) = receiver.recv().await {
            assert_eq!("Hello World", res)
        }
    }
}

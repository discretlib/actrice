use std::thread;

use crate::Receiver;
use crate::Sender;
use crate::channel;

use tokio::sync::oneshot;

pub trait Actor: Sized + Send + 'static {
    fn start(self) -> Context<Self> {
        Context::new(self)
    }
}

pub trait SyncHandler<M, R>
where
    R: Send + 'static,
{
    fn handle(&mut self, msg: M) -> R;
}

pub trait SyncChannelHandler<M, R> {
    fn handle(&mut self, msg: M, sender: &Sender<R>);
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

        thread::spawn(move || {
            while let Ok(fun) = receiv.recv() {
                let mut message = fun.0;
                message.handle(&mut act);
            }
        });

        Context { sender }
    }

    pub fn send<M, R>(&self, msg: M) -> R
    where
        M: Send + 'static,
        R: Send + 'static,
        A: SyncHandler<M, R>,
    {
        let (tx, rx) = oneshot::channel();
        let proxy: SyncEnveloppeProxy<M, R> = SyncEnveloppeProxy {
            msg: Some(msg),
            sender: Some(tx),
        };

        self.sender.send(Enveloppe(Box::new(proxy))).unwrap();
        rx.blocking_recv().unwrap()
    }

    pub async fn send_async<M, R>(&self, msg: M) -> R
    where
        M: Send + 'static,
        R: Send + 'static,
        A: SyncHandler<M, R>,
    {
        let (tx, rx) = oneshot::channel();
        let proxy: SyncEnveloppeProxy<M, R> = SyncEnveloppeProxy {
            msg: Some(msg),
            sender: Some(tx),
        };

        self.sender.send(Enveloppe(Box::new(proxy))).unwrap();
        rx.await.unwrap()
    }

    pub fn send_with_channel<M, R>(&self, msg: M) -> Receiver<R>
    where
        M: Send + 'static,
        R: Send + 'static,
        A: SyncChannelHandler<M, R>,
    {
        let (send, receiv) = channel(1);
        let proxy: SyncChannelEnveloppeProxy<M, R> = SyncChannelEnveloppeProxy {
            msg: Some(msg),
            sender: send,
        };
        self.sender.send(Enveloppe(Box::new(proxy))).unwrap();
        receiv
    }

    pub async fn send_with_channel_async<M, R>(&self, msg: M) -> Receiver<R>
    where
        M: Send + 'static,
        R: Send + 'static,
        A: SyncChannelHandler<M, R>,
    {
        let (send, receiv) = channel(1);
        let proxy: SyncChannelEnveloppeProxy<M, R> = SyncChannelEnveloppeProxy {
            msg: Some(msg),
            sender: send,
        };
        self.sender
            .send_async(Enveloppe(Box::new(proxy)))
            .await
            .unwrap();
        receiv
    }

    pub fn send_with_provided_channel<M, R>(&self, msg: M, sender: Sender<R>)
    where
        M: Send + 'static,
        R: Send + 'static,
        A: SyncChannelHandler<M, R>,
    {
        let proxy: SyncChannelEnveloppeProxy<M, R> = SyncChannelEnveloppeProxy {
            msg: Some(msg),
            sender,
        };
        self.sender.send(Enveloppe(Box::new(proxy))).unwrap();
    }

    pub async fn send_with_provided_channel_async<M, R>(&self, msg: M, sender: Sender<R>)
    where
        M: Send + 'static,
        R: Send + 'static,
        A: SyncChannelHandler<M, R>,
    {
        let proxy: SyncChannelEnveloppeProxy<M, R> = SyncChannelEnveloppeProxy {
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

trait EnveloppeProxy<A> {
    fn handle(&mut self, act: &mut A);
}

struct SyncEnveloppeProxy<M, R>
where
    M: Send + 'static,
    R: Send + 'static,
{
    msg: Option<M>,
    sender: Option<oneshot::Sender<R>>,
}

impl<A, M, R> EnveloppeProxy<A> for SyncEnveloppeProxy<M, R>
where
    M: Send + 'static,
    R: Send + 'static,
    A: Actor + SyncHandler<M, R> + Send + 'static,
{
    fn handle(&mut self, act: &mut A) {
        let result = act.handle(self.msg.take().unwrap());
        let senser = self.sender.take().unwrap();
        let _ = senser.send(result);
    }
}

struct SyncChannelEnveloppeProxy<M, R>
where
    M: Send + 'static,
    R: Send + 'static,
{
    msg: Option<M>,
    sender: Sender<R>,
}

impl<A, M, R> EnveloppeProxy<A> for SyncChannelEnveloppeProxy<M, R>
where
    M: Send + 'static,
    R: Send + 'static,
    A: Actor + SyncChannelHandler<M, R> + Send + 'static,
{
    fn handle(&mut self, act: &mut A) {
        act.handle(self.msg.take().unwrap(), &self.sender);
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn send() {
        struct Greeter;
        impl Actor for Greeter {}

        impl SyncHandler<String, String> for Greeter {
            fn handle(&mut self, msg: String) -> String {
                format!("Hello {msg}")
            }
        }

        let ctx = Greeter.start();
        let res: String = ctx.send("World".to_owned());

        assert_eq!("Hello World", res)
    }

    #[test]
    fn send_channel() {
        struct Repeat {
            value: String,
            number: usize,
        }

        struct Repeater;
        impl Actor for Repeater {}

        impl SyncChannelHandler<Repeat, String> for Repeater {
            fn handle(&mut self, msg: Repeat, sender: &Sender<String>) {
                for _ in 0..msg.number {
                    sender.send(msg.value.to_owned()).unwrap();
                }
            }
        }

        let ctx = Repeater.start();
        let number = 10;

        let chan = ctx.send_with_channel(Repeat {
            value: "Hello World".to_owned(),
            number: number,
        });
        let mut expected = 0;
        while let Ok(_) = chan.recv() {
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
        );
        let mut expected = 0;
        while let Ok(_) = receiv.recv() {
            expected += 1;
        }
        assert_eq!(number, expected);
    }

    #[tokio::test]
    async fn async_send() {
        struct Greeter;
        impl Actor for Greeter {}

        impl SyncHandler<String, String> for Greeter {
            fn handle(&mut self, msg: String) -> String {
                format!("Hello {msg}")
            }
        }

        let ctx = Greeter.start();
        let res: String = ctx.send_async("World".to_owned()).await;

        assert_eq!("Hello World", res)
    }

    #[tokio::test]
    async fn async_send_channel() {
        struct Repeat {
            value: String,
            number: usize,
        }

        struct Repeater;
        impl Actor for Repeater {}

        impl SyncChannelHandler<Repeat, String> for Repeater {
            fn handle(&mut self, msg: Repeat, sender: &Sender<String>) {
                for _ in 0..msg.number {
                    sender.send(msg.value.to_owned()).unwrap();
                }
            }
        }

        let ctx = Repeater.start();
        let number = 10;

        let chan = ctx
            .send_with_channel_async(Repeat {
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
        ctx.send_with_provided_channel_async(
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

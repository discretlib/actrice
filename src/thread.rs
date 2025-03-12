use std::num::NonZero;
use std::thread;

use crate::Receiver;
use crate::Sender;
use crate::channel;

use tokio::sync::oneshot;

pub trait ActorThreaded: Sized + Send + 'static {
    fn start(self) -> Adress<Self> {
        Adress::new(self)
    }

    fn start_multi_threaded(self, num_threads: NonZero<usize>) -> Adress<Self>
    where
        Self: Clone,
    {
        Adress::new_multi_thread(self, num_threads.get())
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
pub struct Adress<A>
where
    A: ActorThreaded + Send + 'static,
{
    sender: Sender<Enveloppe<A>>,
}
impl<A: ActorThreaded + Send + 'static> Adress<A> {
    fn new(mut act: A) -> Adress<A> {
        let (sender, receiv) = channel::<Enveloppe<A>>(1);

        thread::spawn(move || {
            while let Some(fun) = receiv.recv_sync() {
                let mut message = fun.0;
                message.handle(&mut act);
            }
        });

        Adress { sender }
    }

    fn new_multi_thread(act: A, num_threads: usize) -> Adress<A>
    where
        A: Clone,
    {
        let (sender, receiv) = channel::<Enveloppe<A>>(num_threads);

        for _i in 0..num_threads {
            let receiv = receiv.clone();
            let mut act = act.clone();
            thread::spawn(move || {
                while let Some(fun) = receiv.recv_sync() {
                    let mut message = fun.0;
                    message.handle(&mut act);
                }
            });
        }

        Adress { sender }
    }

    pub async fn ask<M, R>(&self, msg: M) -> R
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

        self.sender.send(Enveloppe(Box::new(proxy))).await;
        rx.await.unwrap()
    }

    pub fn ask_sync<M, R>(&self, msg: M) -> R
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

        self.sender.send_sync(Enveloppe(Box::new(proxy)));
        rx.blocking_recv().unwrap()
    }

    pub async fn ask_channel<M, R>(&self, msg: M) -> Receiver<R>
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
        self.sender.send(Enveloppe(Box::new(proxy))).await;
        receiv
    }

    pub fn ask_channel_sync<M, R>(&self, msg: M) -> Receiver<R>
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
        self.sender.send_sync(Enveloppe(Box::new(proxy)));
        receiv
    }

    pub async fn ask_with<M, R>(&self, msg: M, sender: Sender<R>)
    where
        M: Send + 'static,
        R: Send + 'static,
        A: SyncChannelHandler<M, R>,
    {
        let proxy: SyncChannelEnveloppeProxy<M, R> = SyncChannelEnveloppeProxy {
            msg: Some(msg),
            sender,
        };
        self.sender.send(Enveloppe(Box::new(proxy))).await;
    }

    pub fn ask_with_sync<M, R>(&self, msg: M, sender: Sender<R>)
    where
        M: Send + 'static,
        R: Send + 'static,
        A: SyncChannelHandler<M, R>,
    {
        let proxy: SyncChannelEnveloppeProxy<M, R> = SyncChannelEnveloppeProxy {
            msg: Some(msg),
            sender,
        };
        self.sender.send_sync(Enveloppe(Box::new(proxy)));
    }

    pub async fn send<M, R>(&self, msg: M)
    where
        M: Send + 'static,
        R: Send + 'static,
        A: SyncHandler<M, R>,
    {
        let proxy: SyncEnveloppeProxy<M, R> = SyncEnveloppeProxy {
            msg: Some(msg),
            sender: None,
        };

        self.sender.send(Enveloppe(Box::new(proxy))).await;
    }

    pub fn send_sync<M, R>(&self, msg: M)
    where
        M: Send + 'static,
        R: Send + 'static,
        A: SyncHandler<M, R>,
    {
        let proxy: SyncEnveloppeProxy<M, R> = SyncEnveloppeProxy {
            msg: Some(msg),
            sender: None,
        };

        self.sender.send_sync(Enveloppe(Box::new(proxy)));
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
    A: ActorThreaded + SyncHandler<M, R> + Send + 'static,
{
    fn handle(&mut self, act: &mut A) {
        let result = act.handle(self.msg.take().unwrap());
        if let Some(sender) = self.sender.take() {
            let _ = sender.send(result);
        }
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
    A: ActorThreaded + SyncChannelHandler<M, R> + Send + 'static,
{
    fn handle(&mut self, act: &mut A) {
        act.handle(self.msg.take().unwrap(), &self.sender);
    }
}

#[cfg(test)]
mod test {
    use std::time;

    use super::*;

    #[tokio::test]
    async fn ask() {
        struct Greeter;
        impl ActorThreaded for Greeter {}

        impl SyncHandler<String, String> for Greeter {
            fn handle(&mut self, msg: String) -> String {
                format!("Hello {msg}")
            }
        }

        let ctx = Greeter.start();
        let res: String = ctx.ask("World".to_owned()).await;

        assert_eq!("Hello World", res)
    }

    #[test]
    fn ask_sync() {
        struct Greeter;
        impl ActorThreaded for Greeter {}

        impl SyncHandler<String, String> for Greeter {
            fn handle(&mut self, msg: String) -> String {
                format!("Hello {msg}")
            }
        }

        let ctx = Greeter.start();
        let res: String = ctx.ask_sync("World".to_owned());

        assert_eq!("Hello World", res)
    }

    #[tokio::test]
    async fn ask_channel() {
        struct Repeat {
            value: String,
            number: usize,
        }

        struct Repeater;
        impl ActorThreaded for Repeater {}

        impl SyncChannelHandler<Repeat, String> for Repeater {
            fn handle(&mut self, msg: Repeat, sender: &Sender<String>) {
                for _ in 0..msg.number {
                    sender.send_sync(msg.value.to_owned());
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

    #[test]
    fn ask_channel_sync() {
        struct Repeat {
            value: String,
            number: usize,
        }

        struct Repeater;
        impl ActorThreaded for Repeater {}

        impl SyncChannelHandler<Repeat, String> for Repeater {
            fn handle(&mut self, msg: Repeat, sender: &Sender<String>) {
                for _ in 0..msg.number {
                    sender.send_sync(msg.value.to_owned());
                }
            }
        }

        let ctx = Repeater.start();
        let number = 10;

        let chan = ctx.ask_channel_sync(Repeat {
            value: "Hello World".to_owned(),
            number: number,
        });
        let mut expected = 0;
        while let Some(_) = chan.recv_sync() {
            expected += 1;
        }
        assert_eq!(number, expected);

        let (send, receiv) = channel(10);
        ctx.ask_with_sync(
            Repeat {
                value: "Hello World".to_owned(),
                number: number,
            },
            send,
        );
        let mut expected = 0;
        while let Some(_) = receiv.recv_sync() {
            expected += 1;
        }
        assert_eq!(number, expected);
    }

    #[test]
    fn ask_multithread() {
        const SLEEP_DURATION: u64 = 10;

        #[derive(Clone)]
        struct CPUIntensive;
        impl ActorThreaded for CPUIntensive {}

        impl SyncChannelHandler<String, ()> for CPUIntensive {
            fn handle(&mut self, _: String, sender: &Sender<()>) {
                let ten_millis = time::Duration::from_millis(SLEEP_DURATION);

                thread::sleep(ten_millis);

                sender.send_sync(())
            }
        }
        let num_treads = 4;
        let ctx = CPUIntensive.start_multi_threaded(NonZero::new(num_treads).unwrap());
        let now = time::Instant::now();
        let (send, receiv) = channel(10);
        for _ in 0..num_treads {
            ctx.ask_with_sync("Hello World".to_owned(), send.clone());
        }
        //drop the original sender  to allow the receveiver to end
        drop(send);
        let mut expected = 0;
        while let Some(_) = receiv.recv_sync() {
            expected += 1;
        }
        assert_eq!(num_treads, expected);
        let millies = now.elapsed().as_millis() as u64;
        assert!(millies < SLEEP_DURATION + 1);
    }

    #[tokio::test]
    async fn send() {
        struct Greeter {
            sender: Sender<String>,
        }
        impl ActorThreaded for Greeter {}

        impl SyncHandler<String, ()> for Greeter {
            fn handle(&mut self, msg: String) -> () {
                self.sender.send_sync(format!("Hello {msg}"))
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

    #[test]
    fn send_sync() {
        struct Greeter {
            sender: Sender<String>,
        }
        impl ActorThreaded for Greeter {}

        impl SyncHandler<String, ()> for Greeter {
            fn handle(&mut self, msg: String) -> () {
                self.sender.send_sync(format!("Hello {msg}"))
            }
        }

        let (sender, receiver) = channel(1);
        let greeter = Greeter { sender };
        let ctx = greeter.start();
        ctx.send_sync("World".to_owned());
        if let Some(res) = receiver.recv_sync() {
            assert_eq!("Hello World", res)
        }
    }
}

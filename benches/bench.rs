use actix::{Actor as ActixActor, Context, Handler, Message};
use actrice::{
    asynch::{Actor, AsyncHandler},
    thread::{ActorThreaded, SyncHandler},
};
use criterion::{Criterion, criterion_group, criterion_main};
use pprof::criterion::{Output, PProfProfiler};

//
// run benchmark with
// - cargo bench
//
// creating a flamegraph requires setting a kernel parameter
// - echo -1 | sudo tee /proc/sys/kernel/perf_event_paranoid
// then launch:
// - cargo bench --bench bench -- --profile-time=10
// the flamegraph.svg can be found
//    - target/criterion/bench/Acteur/profile/
//

criterion_main!(benches);

criterion_group! {
    name = benches;
    config = Criterion::default().with_profiler(PProfProfiler::new(1000, Output::Flamegraph(None)));
    targets = benchmark
}
const NUM_MESSAGE: u32 = 1000;

struct Sum {
    inc: usize,
}
impl Actor for Sum {}
impl AsyncHandler<u32, usize> for Sum {
    async fn handle(&mut self, msg: u32) -> usize {
        self.inc += msg as usize;
        self.inc
    }
}

struct ThreadedSum {
    inc: usize,
}

impl ActorThreaded for ThreadedSum {}

impl SyncHandler<u32, usize> for ThreadedSum {
    fn handle(&mut self, msg: u32) -> usize {
        self.inc += msg as usize;
        self.inc
    }
}

#[derive(Message)]
#[rtype(usize)]
struct SumMsg(u32);

struct ActixSum {
    inc: usize,
}
impl ActixActor for ActixSum {
    type Context = Context<Self>;
}

impl Handler<SumMsg> for ActixSum {
    type Result = usize; // <- Message response type

    fn handle(&mut self, msg: SumMsg, _ctx: &mut Context<Self>) -> Self::Result {
        self.inc += msg.0 as usize;
        self.inc
    }
}

async fn run() {
    let sum = Sum { inc: 0 };
    let adress = sum.start();

    let mut res = 0;
    for i in 0..NUM_MESSAGE {
        res += adress.ask(i).await;
    }
    assert!(res > 0)
}

fn run_threaded() {
    let sum = ThreadedSum { inc: 0 };
    let adress = sum.start();

    let mut res = 0;
    for i in 0..NUM_MESSAGE {
        res += adress.ask_sync(i);
    }
    assert!(res > 0)
}

async fn run_actix() {
    let sum = ActixSum { inc: 0 };
    let adress = sum.start();

    let mut res = 0;
    for i in 0..NUM_MESSAGE {
        res += adress.send(SumMsg(i)).await.unwrap();
    }
    assert!(res > 0)
}

pub fn benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("bench");

    group.throughput(criterion::Throughput::Elements(NUM_MESSAGE as u64));

    group.bench_function("Actrice Threaded", |b| {
        b.iter(|| {
            run_threaded();
        })
    });

    let runner = tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap();
    group.bench_function("Actrice Async", |b| b.iter(|| runner.block_on(run())));

    let runner = actix::System::with_tokio_rt(|| {
        tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap()
    });
    group.bench_function("Actix", |b| b.iter(|| runner.block_on(run_actix())));
}

// impl AsyncExecutor for ActixRuntime {
//     fn block_on<T>(&self, future: impl Future<Output = T>) -> T {
//         self.rt.borrow_mut().block_on(future)
//     }
// }

// pub struct ActixRuntime {
//     rt: RefCell<actix::SystemRunner>,
// }

// impl ActixRuntime {
//     pub fn new() -> Self {
//         ActixRuntime {
//             rt: RefCell::new(actix::System::new()),
//         }
//     }
// }

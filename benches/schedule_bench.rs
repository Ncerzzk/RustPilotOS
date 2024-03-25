use criterion::{black_box, criterion_group, criterion_main, Criterion};
use rpos::pthread_scheduler::SchedulePthread;
use std::ptr::null_mut;
use std::sync::Arc;

/*
    test the latency of thread schedule(from thread func to next hrt wake instant)
*/

fn test_schedule_bench(c: &mut Criterion) {
    fn test(ptr: *mut libc::c_void) -> *mut libc::c_void {
        let mut cnt = 0;
        let sp = unsafe { Arc::from_raw(ptr as *const SchedulePthread) };
        let mut c = unsafe { &mut *(sp.thread_args as *mut Criterion) };

        c.bench_function("test_schedule_bench", move |b| {
            b.iter_custom(|iters| {
                let mut sum_latency = 0;
                for i in 0..iters {
                    let latency =
                        *(sp.last_scheduled_time.read().unwrap()) - *(sp.deadline.read().unwrap());
                    let latency = (latency.to_nano() / 1000) as u64;
                    if latency > 500000 {
                        println!(
                            "{:?}   ,   {:?}",
                            *(sp.last_scheduled_time.read().unwrap()),
                            *(sp.deadline.read().unwrap())
                        );
                        panic!("failed");
                    }
                    sum_latency += latency;
                    sp.schedule_until(2500);
                }
                std::time::Duration::from_micros(sum_latency)
            })
        });
        null_mut()
    }

    let sp = SchedulePthread::new(
        16384,
        98,
        test,
        c as *mut Criterion as *mut libc::c_void,
        true,
        None,
    );

    sp.join();
}

/*
    test the real latency of thread schedule(from thread func to thread func)
*/
fn test_schedule_bench_real_100us(c: &mut Criterion) {
    fn test(ptr: *mut libc::c_void) -> *mut libc::c_void {
        let mut cnt = 0;
        let sp = unsafe { Arc::from_raw(ptr as *const SchedulePthread) };
        let mut c = unsafe { &mut *(sp.thread_args as *mut Criterion) };

        c.bench_function("test_schedule_bench_real", move |b| {
            b.iter(|| {
                    sp.schedule_after(100);
            });
        });
        null_mut()
    }

    unsafe {assert_eq!(libc::mlockall(1 | 2),0)};
    
    let sp = SchedulePthread::new(
        16384,
        98,
        test,
        c as *mut Criterion as *mut libc::c_void,
        true,
        None,
    );

    sp.join();
}

/*
    test the latency between hrt wake to func start to execute
*/
fn test_schedule_bench_hrt2call(c: &mut Criterion) {
    fn test(ptr: *mut libc::c_void) -> *mut libc::c_void {
        let mut cnt = 0;
        let sp = unsafe { Arc::from_raw(ptr as *const SchedulePthread) };
        let mut c = unsafe { &mut *(sp.thread_args as *mut Criterion) };

        c.bench_function("test_schedule_bench_hrt2call,", move |b| {
            b.iter_custom(|iters| {
                let mut sum_latency = 0;
                for i in 0..iters {
                    sp.schedule_until(2500);
                    let latency = rpos::hrt::get_time_now() - *(sp.last_scheduled_time.read().unwrap());
                    sum_latency += latency.to_nano();
                }
                std::time::Duration::from_micros((sum_latency / 1000) as u64)
            })
        });
        null_mut()
    }

    let sp = SchedulePthread::new(
        16384,
        98,
        test,
        c as *mut Criterion as *mut libc::c_void,
        true,
        None,
    );

    sp.join();
}

criterion_group!(benches, test_schedule_bench,test_schedule_bench_hrt2call,test_schedule_bench_real_100us);

criterion_main!(benches);

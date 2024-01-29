extern crate criterion;
use criterion::{black_box, criterion_group, criterion_main, Criterion};


fn test_schedule_bench(c: &mut Criterion){ 
    println!("hello");
}

criterion_group!(benches, test_schedule_bench);
criterion_main!(benches);
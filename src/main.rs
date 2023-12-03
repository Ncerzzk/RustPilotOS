use std::{sync::{Mutex, Condvar, Arc, Weak}, collections::VecDeque, thread::{spawn}, borrow::BorrowMut, time::{Duration, SystemTime}, mem::MaybeUninit};
mod pthread;
mod workqueue;
mod hrt;

fn main() {
    println!("Hello, world!");
}

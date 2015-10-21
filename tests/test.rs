#![feature(catch_panic)]
extern crate bytes;
extern crate mio;
extern crate time;

extern crate coros;

use std::thread;

use bytes::SliceBuf;
use mio::*;
use time::{
    now,
    Duration
};

use coros::Pool;
use coros::CoroutineHandle;
use coros::notifying_channel;

const STACK_SIZE: usize = 2 * 1024 * 1024;

#[test]
fn test_pool() {
    let pool_name = "a_name".to_string();
    let mut pool = Pool::new(pool_name, 1);
    let guard = pool.spawn(|_| { 1 }, STACK_SIZE);
    pool.start().unwrap();

    assert_eq!(1, guard.join().unwrap());

    pool.stop().unwrap();
}

#[test]
fn test_spawning_after_start() {
    let pool_name = "a_name".to_string();
    let mut pool = Pool::new(pool_name, 1);
    pool.start().unwrap();
    let guard = pool.spawn(|_| { 1 }, STACK_SIZE);

    assert_eq!(1, guard.join().unwrap());

    pool.stop().unwrap();
}

#[test]
fn test_spawning_multiple_coroutines() {
    let pool_name = "a_name".to_string();
    let mut pool = Pool::new(pool_name, 1);
    pool.start().unwrap();
    let guard1 = pool.spawn(|_| { 1 }, STACK_SIZE);
    let guard2 = pool.spawn(|_| { 2 }, STACK_SIZE);

    assert_eq!(1, guard1.join().unwrap());
    assert_eq!(2, guard2.join().unwrap());

    pool.stop().unwrap();
}

#[test]
fn test_coroutine_panic() {
    let pool_name = "a_name".to_string();
    let mut pool = Pool::new(pool_name, 1);
    pool.start().unwrap();
    let guard1 = pool.spawn(
        |_| { thread::catch_panic(move || { panic!("panic1") }) },
        STACK_SIZE,
    );
    let guard2 = pool.spawn(
        |_| { thread::catch_panic(move || { panic!("panic2") }) },
        STACK_SIZE,
    );
    let guard4 = pool.spawn(
        |_| { thread::catch_panic(move || { 4 }) },
        STACK_SIZE,
    );
    let guard5 = pool.spawn(|_| { 5 }, STACK_SIZE);
    assert!(guard1.join().unwrap().is_err());
    assert!(guard2.join().unwrap().is_err());
    assert_eq!(4, guard4.join().unwrap().unwrap());
    assert_eq!(5, guard5.join().unwrap());

    pool.stop().unwrap();
    assert!(true);
}

#[test]
fn test_dropping_the_pool_stops_it() {
    let pool_name = "a_name".to_string();
    let mut pool = Pool::new(pool_name, 1);
    pool.spawn(|_| { 1 }, STACK_SIZE);

    pool.start().unwrap();
}

#[test]
fn test_work_stealing() {
    let pool_name = "a_name".to_string();
    let mut pool = Pool::new(pool_name, 2);
    let guard2 = pool.spawn_with_thread_index(|_| { 2 }, STACK_SIZE, 0);
    let guard1 = pool.spawn_with_thread_index(|_| { thread::sleep_ms(500); 1 }, STACK_SIZE, 0);

    let start_time = now();
    pool.start().unwrap();
    assert_eq!(2, guard2.join().unwrap());

    assert!((now() - start_time) < Duration::milliseconds(500));
    assert_eq!(1, guard1.join().unwrap());
    assert!((now() - start_time) >= Duration::milliseconds(500));
    pool.stop().unwrap();
}

#[test]
fn test_nested_coroutines() {
    let outer_pool_name = "outer".to_string();
    let mut outer_pool = Pool::new(outer_pool_name, 2);
    let outer_guard = outer_pool.spawn(
        |_| {
            let inner_pool_name = "inner".to_string();
            let mut inner_pool = Pool::new(inner_pool_name, 2);
            inner_pool.start().unwrap();
            let inner_guard = inner_pool.spawn(|_| { 1 }, STACK_SIZE);
            let inner_result = inner_guard.join().unwrap();
            inner_pool.stop().unwrap();
            inner_result
        },
        STACK_SIZE,
    );

    outer_pool.start().unwrap();
    assert_eq!(1, outer_guard.join().unwrap());
    outer_pool.stop().unwrap();
}

#[test]
fn test_sleep_ms() {
    let pool_name = "a_name".to_string();
    let mut pool = Pool::new(pool_name, 2);
    let guard1 = pool.spawn_with_thread_index(|_| { 1 }, STACK_SIZE, 0);
    let guard2 = pool.spawn_with_thread_index(
        |coroutine_handle: &mut CoroutineHandle| {
            coroutine_handle.sleep_ms(500);
            2
        },
        STACK_SIZE,
        1,
    );

    let start_time = now();
    pool.start().unwrap();
    assert_eq!(1, guard1.join().unwrap());
    assert!((now() - start_time) < Duration::milliseconds(500));
    assert_eq!(2, guard2.join().unwrap());
    assert!((now() - start_time) >= Duration::milliseconds(500));
    pool.stop().unwrap();
}

#[test]
fn test_channel_recv() {
    let pool_name = "a_name".to_string();
    let mut pool = Pool::new(pool_name, 1);
    let (sender, receiver) = notifying_channel::<u8>();
    let guard = pool.spawn(
        move |coroutine_handle: &mut CoroutineHandle| {
            coroutine_handle.recv(&receiver).unwrap()
        },
        STACK_SIZE,
    );

    pool.start().unwrap();
    sender.send(1);
    assert_eq!(1, guard.join().unwrap());
    pool.stop().unwrap();
}

#[test]
fn test_readable_io() {
    let pool_name = "a_name".to_string();
    let mut pool = Pool::new(pool_name, 1);
    let (mut reader, mut writer) = unix::pipe().unwrap();

    let guard = pool.spawn(
        move |coroutine_handle: &mut CoroutineHandle| {
            coroutine_handle.register(&reader, EventSet::readable(), PollOpt::edge());
            let mut result_buf = Vec::<u8>::new();
            reader.try_read_buf(&mut result_buf).unwrap();

            std::str::from_utf8(&result_buf).unwrap().to_string()
        },
        STACK_SIZE,
    );

    writer.try_write_buf(&mut SliceBuf::wrap("ping".as_bytes())).unwrap();


    pool.start().unwrap();
    assert_eq!("ping", guard.join().unwrap());
    pool.stop().unwrap();
}

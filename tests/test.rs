#![feature(recover)]
extern crate bytes;
extern crate mio;
extern crate time;

extern crate coros;

use std::time::Duration as StdDuration;

use bytes::SliceBuf;
use mio::*;
use time::{
    Duration,
    now,
};

use coros::Pool;
use coros::CoroutineBlockingHandle;
use coros::coroutine_channel;

const STACK_SIZE: usize = 2 * 1024 * 1024;

#[test]
fn test_pool() {
    let pool_name = "a_name".to_string();
    let mut pool = Pool::new(pool_name, 1);
    let mut guard = pool.spawn(|_| { 1 }, STACK_SIZE).unwrap();
    pool.start().unwrap();

    assert_eq!(1, guard.join().unwrap().unwrap());

    pool.stop().unwrap();
}

#[test]
fn test_spawning_after_start() {
    let pool_name = "a_name".to_string();
    let mut pool = Pool::new(pool_name, 1);
    pool.start().unwrap();

    let mut guard  = pool.spawn(|_| { 1 }, STACK_SIZE).unwrap();
    assert_eq!(1, guard.join().unwrap().unwrap());

    pool.stop().unwrap();
}

#[test]
fn test_spawning_multiple_coroutines() {
    let pool_name = "a_name".to_string();
    let mut pool = Pool::new(pool_name, 1);
    pool.start().unwrap();
    let mut guard1 = pool.spawn(|_| { 1 }, STACK_SIZE).unwrap();
    let mut guard2 = pool.spawn(|_| { 2 }, STACK_SIZE).unwrap();

    assert_eq!(1, guard1.join().unwrap().unwrap());
    assert_eq!(2, guard2.join().unwrap().unwrap());

    pool.stop().unwrap();
}

#[test]
fn test_coroutine_panic() {
    let pool_name = "a_name".to_string();
    let mut pool = Pool::new(pool_name, 1);
    pool.start().unwrap();
    let mut guard1 = pool.spawn(
        |_| { std::panic::recover(move || { panic!("panic1") }) },
        STACK_SIZE,
    ).unwrap();
    let mut guard2 = pool.spawn(
        |_| { std::panic::recover(move || { panic!("panic2") }) },
        STACK_SIZE,
    ).unwrap();
    let mut guard4 = pool.spawn(
        |_| { std::panic::recover(move || { 4 }) },
        STACK_SIZE,
    ).unwrap();
    let mut guard5 = pool.spawn(|_| { 5 }, STACK_SIZE).unwrap();
    assert!(guard1.join().unwrap().unwrap().is_err());
    assert!(guard2.join().unwrap().unwrap().is_err());
    assert_eq!(4, guard4.join().unwrap().unwrap().unwrap());
    assert_eq!(5, guard5.join().unwrap().unwrap());

    pool.stop().unwrap();
    assert!(true);
}

#[test]
fn test_dropping_the_pool_stops_it() {
    let pool_name = "a_name".to_string();
    let mut pool = Pool::new(pool_name, 1);

    pool.start().unwrap();
    pool.spawn(|_| { 1 }, STACK_SIZE).unwrap().join().unwrap().unwrap();
}

#[test]
#[allow(unused_variables)]
fn test_dropping_a_join_handle_joins_it() {
    let pool_name = "a_name".to_string();
    let mut pool = Pool::new(pool_name, 1);
    let handle = pool.spawn(|_| {
        std::thread::sleep(StdDuration::from_millis(500));
        1
    }, STACK_SIZE).unwrap();

    pool.start().unwrap();
}

#[test]
fn test_work_stealing() {
    let pool_name = "a_name".to_string();
    let mut pool = Pool::new(pool_name, 2);
    let mut guard2 = pool.spawn_with_thread_index(|_| { 2 }, STACK_SIZE, 0).unwrap();
    let mut guard1 = pool.spawn_with_thread_index(|_| { std::thread::sleep(StdDuration::from_millis(500)); 1 }, STACK_SIZE, 0).unwrap();

    let start_time = now();
    pool.start().unwrap();
    assert_eq!(2, guard2.join().unwrap().unwrap());

    assert!((now() - start_time) < Duration::milliseconds(500));
    assert_eq!(1, guard1.join().unwrap().unwrap());
    assert!((now() - start_time) >= Duration::milliseconds(500));
    pool.stop().unwrap();
}

#[test]
fn test_nested_coroutines() {
    let outer_pool_name = "outer".to_string();
    let mut outer_pool = Pool::new(outer_pool_name, 2);
    let mut outer_guard = outer_pool.spawn(
        |_| {
            let inner_pool_name = "inner".to_string();
            let mut inner_pool = Pool::new(inner_pool_name, 2);
            inner_pool.start().unwrap();
            let mut inner_guard = inner_pool.spawn(|_| { 1 }, STACK_SIZE).unwrap();
            let inner_result = inner_guard.join().unwrap().unwrap();
            inner_pool.stop().unwrap();
            inner_result
        },
        STACK_SIZE,
    ).unwrap();

    outer_pool.start().unwrap();
    assert_eq!(1, outer_guard.join().unwrap().unwrap());
    outer_pool.stop().unwrap();
}

#[test]
fn test_sleep_ms() {
    let pool_name = "a_name".to_string();
    let mut pool = Pool::new(pool_name, 2);
    let mut guard1 = pool.spawn_with_thread_index(|_| { 1 }, STACK_SIZE, 0).unwrap();
    let mut guard2 = pool.spawn_with_thread_index(
        |coroutine_handle: &mut CoroutineBlockingHandle| {
            coroutine_handle.sleep_ms(500);
            2
        },
        STACK_SIZE,
        1,
    ).unwrap();

    let start_time = now();
    pool.start().unwrap();
    assert_eq!(1, guard1.join().unwrap().unwrap());
    assert!((now() - start_time) < Duration::milliseconds(500));
    assert_eq!(2, guard2.join().unwrap().unwrap());
    assert!((now() - start_time) >= Duration::milliseconds(500));
    pool.stop().unwrap();
}

#[test]
fn test_sleeping_coroutine_is_not_awoken_for_io() {
    let pool_name = "a_name".to_string();
    let mut pool = Pool::new(pool_name, 1);
    let (reader, mut writer) = unix::pipe().unwrap();

    let mut guard = pool.spawn(
        move |coroutine_handle: &mut CoroutineBlockingHandle| {
            coroutine_handle.register(
                &reader,
                EventSet::readable(),
                PollOpt::level(),
            );
            let start_time = now();
            coroutine_handle.sleep_ms(500);
            assert!((now() - start_time) >= Duration::milliseconds(400));
            coroutine_handle.deregister(&reader);
        },
        STACK_SIZE,
    ).unwrap();

    pool.start().unwrap();
    writer.try_write_buf(&mut SliceBuf::wrap("ping".as_bytes())).unwrap();
    guard.join().unwrap().unwrap();
    pool.stop().unwrap();
}

#[test]
fn test_channel_recv() {
    let pool_name = "a_name".to_string();
    let mut pool = Pool::new(pool_name, 1);
    let (sender, receiver) = coroutine_channel::<u8>();
    let mut guard = pool.spawn(
        move |coroutine_handle: &mut CoroutineBlockingHandle| {
            coroutine_handle.recv(&receiver).unwrap()
        },
        STACK_SIZE,
    ).unwrap();

    pool.start().unwrap();
    sender.send(1);
    assert_eq!(1, guard.join().unwrap().unwrap());
    pool.stop().unwrap();
}

#[test]
fn test_readable_io() {
    let pool_name = "a_name".to_string();
    let mut pool = Pool::new(pool_name, 1);
    let (mut reader, mut writer) = unix::pipe().unwrap();

    let mut guard = pool.spawn(
        move |coroutine_handle: &mut CoroutineBlockingHandle| {
            coroutine_handle.register(
                &reader,
                EventSet::readable(),
                PollOpt::edge(),
            );
            let mut result_buf = Vec::<u8>::new();
            reader.try_read_buf(&mut result_buf).unwrap();

            std::str::from_utf8(&result_buf).unwrap().to_string()
        },
        STACK_SIZE,
    ).unwrap();

    writer.try_write_buf(&mut SliceBuf::wrap("ping".as_bytes())).unwrap();


    pool.start().unwrap();
    assert_eq!("ping", guard.join().unwrap().unwrap());
    pool.stop().unwrap();
}

#[test]
fn test_eventset_of_result_is_returned_by_register() {
    let pool_name = "a_name".to_string();
    let mut pool = Pool::new(pool_name, 1);
    let (mut reader, mut writer) = unix::pipe().unwrap();

    let mut guard = pool.spawn(
        move |coroutine_handle: &mut CoroutineBlockingHandle| {
            let result_eventset = coroutine_handle.register(
                &reader,
                EventSet::readable(),
                PollOpt::edge(),
            );
            assert_eq!(result_eventset, EventSet::readable());

            let mut result_buf = Vec::<u8>::new();
            reader.try_read_buf(&mut result_buf).unwrap();

            std::str::from_utf8(&result_buf).unwrap().to_string()
        },
        STACK_SIZE,
    ).unwrap();

    writer.try_write_buf(&mut SliceBuf::wrap("ping".as_bytes())).unwrap();


    pool.start().unwrap();
    assert_eq!("ping", guard.join().unwrap().unwrap());
    pool.stop().unwrap();
}

#[test]
fn test_writable_io() {
    let pool_name = "a_name".to_string();
    let mut pool = Pool::new(pool_name, 1);
    let (mut reader, mut writer) = unix::pipe().unwrap();

    let mut guard = pool.spawn(
        move |coroutine_handle: &mut CoroutineBlockingHandle| {
            let result_eventset = coroutine_handle.register(
                &writer,
                EventSet::writable(),
                PollOpt::edge(),
            );
            assert_eq!(result_eventset, EventSet::writable());

            writer.try_write_buf(&mut SliceBuf::wrap("ping".as_bytes())).unwrap();
        },
        STACK_SIZE,
    ).unwrap();
    pool.start().unwrap();
    guard.join().unwrap().unwrap();

    let mut result_buf = Vec::<u8>::new();
    reader.try_read_buf(&mut result_buf).unwrap();

    let result = std::str::from_utf8(&result_buf).unwrap().to_string();

    assert_eq!("ping", result);
    pool.stop().unwrap();
}

#[test]
fn test_deregister() {
    let pool_name = "a_name".to_string();
    let mut pool = Pool::new(pool_name, 1);
    let (reader1, mut writer1) = unix::pipe().unwrap();
    let (mut reader2, mut writer2) = unix::pipe().unwrap();

    let mut guard = pool.spawn(
        move |coroutine_handle: &mut CoroutineBlockingHandle| {
            coroutine_handle.register(
                &reader1,
                EventSet::readable(),
                PollOpt::level(),
            );

            coroutine_handle.deregister(&reader1);

            let awoken_for_eventset = coroutine_handle.register(
                &writer2,
                EventSet::writable(),
                PollOpt::edge(),
            );
            assert_eq!(awoken_for_eventset, EventSet::writable());

            writer2.try_write_buf(&mut SliceBuf::wrap("pong".as_bytes())).unwrap();
        },
        STACK_SIZE,
    ).unwrap();

    pool.start().unwrap();
    writer1.try_write_buf(&mut SliceBuf::wrap("ping".as_bytes())).unwrap();

    guard.join().unwrap().unwrap();

    let mut read_result_buf = Vec::<u8>::new();
    reader2.try_read_buf(&mut read_result_buf).unwrap();
    let read_result = std::str::from_utf8(&read_result_buf).unwrap().to_string();
    assert_eq!("pong", read_result);

    pool.stop().unwrap();
}

#[test]
fn test_reregister() {
    let pool_name = "a_name".to_string();
    let mut pool = Pool::new(pool_name, 1);
    let (mut reader, mut writer) = unix::pipe().unwrap();

    let mut guard = pool.spawn(
        move |coroutine_handle: &mut CoroutineBlockingHandle| {
            coroutine_handle.register(
                &reader,
                EventSet::readable(),
                PollOpt::level(),
            );
            coroutine_handle.deregister(&reader);
            let result_eventset = coroutine_handle.reregister(
                &reader,
                EventSet::readable(),
                PollOpt::level(),
            );
            assert_eq!(result_eventset, EventSet::readable());

            let mut result_buf = Vec::<u8>::new();
            reader.try_read_buf(&mut result_buf).unwrap();

            let read = std::str::from_utf8(&result_buf).unwrap().to_string();
            coroutine_handle.deregister(&reader);

            read
        },
        STACK_SIZE,
    ).unwrap();

    pool.start().unwrap();
    writer.try_write_buf(&mut SliceBuf::wrap("ping".as_bytes())).unwrap();
    assert_eq!(guard.join().unwrap().unwrap(), "ping");
    pool.stop().unwrap();
}

#[test]
fn test_blocked_coroutines_are_waited_on_by_pool_stop() {
    let pool_name = "a_name".to_string();
    let mut pool = Pool::new(pool_name, 1);

    let mut guard = pool.spawn(
        move |coroutine_handle: &mut CoroutineBlockingHandle| {
            coroutine_handle.sleep_ms(500);
            1
        },
        STACK_SIZE,
    ).unwrap();

    pool.start().unwrap();
    std::thread::sleep(StdDuration::from_millis(100));
    pool.stop().unwrap();
    assert_eq!(1, guard.join().unwrap().unwrap());
}

#[test]
fn test_multiple_pool_starts_is_ok() {
    let pool_name = "a_name".to_string();
    let mut pool = Pool::new(pool_name, 1);

    let mut guard = pool.spawn(
        move |_: &mut CoroutineBlockingHandle| { 1 },
        STACK_SIZE,
    ).unwrap();

    pool.start().unwrap();
    pool.start().unwrap();
    assert_eq!(1, guard.join().unwrap().unwrap());
    pool.stop().unwrap();
}

#[test]
fn test_multiple_pool_stops_is_ok() {
    let pool_name = "a_name".to_string();
    let mut pool = Pool::new(pool_name, 1);

    let mut guard = pool.spawn(
        move |_: &mut CoroutineBlockingHandle| { 1 },
        STACK_SIZE,
    ).unwrap();

    pool.start().unwrap();
    assert_eq!(1, guard.join().unwrap().unwrap());
    pool.stop().unwrap();
    pool.stop().unwrap();
}


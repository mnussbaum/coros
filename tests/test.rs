extern crate time;

extern crate coros;

use std::thread;

use time::{
    now,
    Duration
};

use coros::Pool;

#[test]
fn test_pool() {
    let pool_name = "a_name".to_string();
    let pool = Pool::new(pool_name, 1);
    let guard = pool.spawn(|| 1);
    pool.start().unwrap();

    assert_eq!(1, guard.join().unwrap().unwrap());

    pool.stop().unwrap();
}

#[test]
fn test_spawning_after_start() {
    let pool_name = "a_name".to_string();
    let pool = Pool::new(pool_name, 1);
    pool.start().unwrap();
    let guard = pool.spawn(|| 1);

    assert_eq!(1, guard.join().unwrap().unwrap());

    pool.stop().unwrap();
}

#[test]
fn test_spawning_multiple_coroutines() {
    let pool_name = "a_name".to_string();
    let pool = Pool::new(pool_name, 1);
    pool.start().unwrap();
    let guard1 = pool.spawn(|| 1);
    let guard2 = pool.spawn(|| 2);

    assert_eq!(1, guard1.join().unwrap().unwrap());
    assert_eq!(2, guard2.join().unwrap().unwrap());

    pool.stop().unwrap();
}

#[test]
fn test_coroutine_panic() {
    let pool_name = "a_name".to_string();
    let pool = Pool::new(pool_name, 1);
    pool.start().unwrap();
    let guard1 = pool.spawn(|| panic!("panic1") );
    let guard2 = pool.spawn(|| panic!("panic2") );
    let guard3 = pool.spawn(|| panic!("panic3") );
    let guard4 = pool.spawn(|| 4 );
    let guard5 = pool.spawn(|| 5 );
    assert!(guard1.join().unwrap().is_err());
    assert!(guard2.join().unwrap().is_err());
    assert!(guard3.join().unwrap().is_err());
    assert_eq!(4, guard4.join().unwrap().unwrap());
    assert_eq!(5, guard5.join().unwrap().unwrap());

    pool.stop().unwrap();
    assert!(true);
}

#[test]
fn test_dropping_the_pool_stops_it() {
    let pool_name = "a_name".to_string();
    let pool = Pool::new(pool_name, 1);
    pool.spawn(|| 1 );

    pool.start().unwrap();
}

#[test]
fn test_work_stealing() {
    let pool_name = "a_name".to_string();
    let pool = Pool::new(pool_name, 2);
    let guard2 = pool.spawn_with_thread_index(|| { 2 }, 0);
    let guard1 = pool.spawn_with_thread_index(|| { thread::sleep_ms(500); 1 }, 0);

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
    let outer_pool = Pool::new(outer_pool_name, 2);
    let outer_guard = outer_pool.spawn(|| {
            let inner_pool_name = "inner".to_string();
            let inner_pool = Pool::new(inner_pool_name, 2);
            inner_pool.start().unwrap();
            let inner_guard = inner_pool.spawn(|| { 1 });
            let inner_result = inner_guard.join().unwrap().unwrap();
            inner_pool.stop().unwrap();
            inner_result
        });

    outer_pool.start().unwrap();
    assert_eq!(1, outer_guard.join().unwrap().unwrap());
    outer_pool.stop().unwrap();
}

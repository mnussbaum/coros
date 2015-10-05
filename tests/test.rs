extern crate coros;

use std::thread;
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
    assert_eq!(4, guard4.join().unwrap().unwrap());
    assert_eq!(5, guard5.join().unwrap().unwrap());

    pool.stop().unwrap();
    assert!(true);
}

#[test]
fn test_dropping_the_pool_stops_it() {
    let pool_name = "a_name".to_string();
    let pool = Pool::new(pool_name, 1);
    let guard = pool.spawn(|| 1 );

    pool.start().unwrap();
}

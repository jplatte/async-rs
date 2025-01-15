use eyeball::Observable;
use eyeball_im::{ObservableVector, VectorDiff};
use eyeball_im_util::vector::VectorObserverExt;
use imbl::vector;
use stream_assert::{assert_closed, assert_next_eq, assert_pending};

#[test]
fn static_limit() {
    let mut ob: ObservableVector<usize> = ObservableVector::from(vector![1, 20, 300]);
    let (limited, mut sub) = ob.subscribe().rlimit(2);
    assert_eq!(limited, vector![20, 300]);
    assert_pending!(sub);

    // `1` is removed, it's outside the limit window, we get nothing on `sub`.
    ob.pop_front();
    assert_pending!(sub);

    // `200` is removed, it's within the limit window, we get a `PopFront` on `sub`.
    ob.pop_front();
    assert_next_eq!(sub, VectorDiff::PopFront);

    assert_pending!(sub);
    drop(ob);
    assert_closed!(sub);
}

#[test]
fn pending_until_limit_emits_a_value() {
    let mut ob: ObservableVector<usize> = ObservableVector::new();
    let mut limit = Observable::new(0);
    let mut sub = ob.subscribe().dynamic_rlimit(Observable::subscribe(&limit));

    // Append new values…
    ob.append(vector![10, 11, 12, 13, 14, 15]);

    // … but it's still pending…
    assert_pending!(sub);

    // … because the `limit` stream didn't produce any value yet.
    // Let's change that.
    Observable::set(&mut limit, 3);

    // Here we are.
    assert_next_eq!(sub, VectorDiff::Append { values: vector![13, 14, 15] });

    assert_pending!(sub);
    drop(ob);
    assert_closed!(sub);
}

#[test]
fn update_limit_before_polling_sub() {
    let mut ob = ObservableVector::<usize>::new();
    let mut limit = Observable::new(0);
    let mut sub = ob.subscribe().dynamic_rlimit(Observable::subscribe(&limit));

    // Append 4 values.
    ob.append(vector![10, 11, 12, 13]);

    // Observe nothing because the limit is still 0.
    assert_pending!(sub);

    // Set limit to 2.
    Observable::set(&mut limit, 2);

    // Observe 2 values.
    assert_next_eq!(sub, VectorDiff::Append { values: vector![12, 13] });

    // Observe nothing.
    assert_pending!(sub);

    drop(ob);
    assert_closed!(sub);
}

#[test]
fn update_limit_after_polling_sub() {
    let mut ob = ObservableVector::<usize>::new();
    let mut limit = Observable::new(0);
    let mut sub = ob.subscribe().dynamic_rlimit(Observable::subscribe(&limit));

    // Append 4 values.
    ob.append(vector![10, 11, 12, 13]);

    // Set limit to 2.
    Observable::set(&mut limit, 2);

    // Observe 2 values.
    assert_next_eq!(sub, VectorDiff::Append { values: vector![12, 13] });

    // Observe nothing.
    assert_pending!(sub);

    drop(ob);
    assert_closed!(sub);
}

#[test]
fn increase_and_decrease_the_limit_on_an_empty_stream() {
    let ob = ObservableVector::<usize>::new();
    let mut limit = Observable::new(1);
    let (limited, mut sub) =
        ob.subscribe().dynamic_rlimit_with_initial_value(1, Observable::subscribe(&limit));

    assert!(limited.is_empty());

    // `ob` is empty!

    // Set limit to 2.
    Observable::set(&mut limit, 2);

    // Observe nothing.
    assert_pending!(sub);

    // Set limit to 1.
    Observable::set(&mut limit, 1);

    // Observe nothing.
    assert_pending!(sub);

    drop(ob);
    assert_closed!(sub);
}

#[test]
fn increase_and_decrease_the_limit_only() {
    let mut ob = ObservableVector::<usize>::new();
    let mut limit = Observable::new(0);
    let mut sub = ob.subscribe().dynamic_rlimit(Observable::subscribe(&limit));

    // Append 4 values.
    ob.append(vector![10, 11, 12, 13]);

    // Observe nothing because the limit is still 0.
    assert_pending!(sub);

    // Set limit to 2.
    Observable::set(&mut limit, 2);

    // Observe 2 values.
    assert_next_eq!(sub, VectorDiff::Append { values: vector![12, 13] });

    // Set limit to 4.
    Observable::set(&mut limit, 4);

    // Observe 2 more values.
    assert_next_eq!(sub, VectorDiff::PushFront { value: 11 });
    assert_next_eq!(sub, VectorDiff::PushFront { value: 10 });

    // Set limit to 6.
    Observable::set(&mut limit, 6);

    // Observe nothing.
    assert_pending!(sub);

    // Set limit to 5.
    Observable::set(&mut limit, 5);

    // Observe nothing.
    assert_pending!(sub);

    // Set limit to 2.
    Observable::set(&mut limit, 2);

    // Observe a truncation.
    assert_next_eq!(sub, VectorDiff::PopFront);
    assert_next_eq!(sub, VectorDiff::PopFront);
    assert_next_eq!(sub, VectorDiff::PopFront);

    // Set limit to 0.
    Observable::set(&mut limit, 0);

    // Observe a `VectorDiff::Clear` is emitted as an optimisation instead of
    // emitting a bunch of `VectorDiff::PopFront`.
    assert_next_eq!(sub, VectorDiff::Clear);

    // Set limit to 5.
    Observable::set(&mut limit, 5);

    // Observe all available values.
    assert_next_eq!(sub, VectorDiff::Append { values: vector![10, 11, 12, 13] });

    assert_pending!(sub);
    drop(ob);
    assert_closed!(sub);
}

#[test]
fn limit_is_zero() {
    let mut ob = ObservableVector::<usize>::new();
    let mut limit = Observable::new(0);
    let mut sub = ob.subscribe().dynamic_rlimit(Observable::subscribe(&limit));

    // Append 4 values.
    ob.append(vector![10, 11, 12, 13]);

    // Set limit to 0.
    Observable::set(&mut limit, 0);

    // Observe nothing.
    assert_pending!(sub);

    // Add 1 value.
    ob.push_back(14);

    // Observe nothing.
    assert_pending!(sub);

    // Set limit to 5.
    Observable::set(&mut limit, 5);

    // Observe 5 values.
    assert_next_eq!(sub, VectorDiff::Append { values: vector![10, 11, 12, 13, 14] });

    // Set limit to 0 again.
    Observable::set(&mut limit, 0);

    // Observe a truncation.
    assert_next_eq!(sub, VectorDiff::Clear);

    // Add 1 value.
    ob.push_back(15);

    // Observe nothing.
    assert_pending!(sub);

    drop(ob);
    assert_closed!(sub);
}

#[test]
fn limit_is_polled_last() {
    let mut ob = ObservableVector::<usize>::new();
    let mut limit = Observable::new(0);
    let mut sub = ob.subscribe().dynamic_rlimit(Observable::subscribe(&limit));

    // Append 4 values.
    ob.append(vector![10, 11, 12, 13]);

    // Set limit to 3.
    Observable::set(&mut limit, 3);

    // Observe 3 values.
    assert_next_eq!(sub, VectorDiff::Append { values: vector![11, 12, 13] });

    // Set limit to 2 _before_…
    Observable::set(&mut limit, 2);

    // … and add 1 value to the back _after_.
    ob.push_back(14);

    // However, let's observe the change of `push_back` _first_…
    assert_next_eq!(sub, VectorDiff::PopFront);
    assert_next_eq!(sub, VectorDiff::PushBack { value: 14 });

    // … and let's observe the updates from the limit change _after_.
    assert_next_eq!(sub, VectorDiff::PopFront);
    assert_pending!(sub);

    drop(ob);
    assert_closed!(sub);
}

#[test]
fn append() {
    let mut ob = ObservableVector::<usize>::new();
    let mut limit = Observable::new(0);
    let mut sub = ob.subscribe().dynamic_rlimit(Observable::subscribe(&limit));

    // We need to test appending values where the number of values compares to limit
    // and the vector's length as follows:
    //
    // | limit | vector's length | test case |
    // |-------|-----------------|-----------|
    // | less  | less            | #1        |
    // | less  | more            | #2        |
    // | more  | less            | #3        |
    // | more  | more            | #4        |

    Observable::set(&mut limit, 4);

    // Force the limit to be polled, otherwise the next `append` will generate a
    // `Append` from the `limit` stream handler, not from the `inner` stream
    // handler.
    assert_pending!(sub);

    // Test case #2.
    //
    // Append less values than the limit, but more values than the vector's length.
    {
        ob.append(vector![10, 11, 12]);

        assert_next_eq!(sub, VectorDiff::Append { values: vector![10, 11, 12] });

        // State of:
        //
        // - the vector: [ 10, 11, 12 ]
        // - the “view”: [ _, 10, 11, 12 ]
    }

    // Test case #4.
    //
    // Append more values than the limit and the vector's length.
    {
        ob.append(vector![13, 14, 15, 16, 17]);

        assert_next_eq!(sub, VectorDiff::PopFront); // 10
        assert_next_eq!(sub, VectorDiff::PopFront); // 11
        assert_next_eq!(sub, VectorDiff::PopFront); // 12
        assert_next_eq!(sub, VectorDiff::Append { values: vector![14, 15, 16, 17] });

        // State of:
        //
        // - the vector: [ 10, 11, 12, 13, 14, 15, 16, 17 ]
        // - the “view”: [ 14, 15, 16, 17 ]
    }

    // Test case #1.
    //
    // Append less values than the limit and the vector's length.
    {
        ob.append(vector![18, 19]);

        assert_next_eq!(sub, VectorDiff::PopFront); // 13
        assert_next_eq!(sub, VectorDiff::PopFront); // 14
        assert_next_eq!(sub, VectorDiff::Append { values: vector![18, 19] });

        // State of:
        //
        // - the vector: [ 10, 11, 12, 13, 14, 15, 16, 17, 18, 19 ]
        // - the “view”: [ 16, 17, 18, 19 ]
    }

    // Test case #3.
    //
    // Append more values than the limit, but less values the vector's length.
    {
        Observable::set(&mut limit, 2);

        assert_next_eq!(sub, VectorDiff::PopFront); // 16
        assert_next_eq!(sub, VectorDiff::PopFront); // 17

        // State of:
        //
        // - the vector: [ 10, 11, 12, 13, 14, 15, 16, 17, 18, 19 ]
        // - the “view”: [ 18, 19 ]

        ob.append(vector![20, 21, 22]);

        assert_next_eq!(sub, VectorDiff::PopFront); // 18
        assert_next_eq!(sub, VectorDiff::PopFront); // 19
        assert_next_eq!(sub, VectorDiff::Append { values: vector![21, 22] });

        // State of:
        //
        // - the vector: [ 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22 ]
        // - the “view”: [ 21, 22 ]
    }

    assert_pending!(sub);

    // Check the content of the vector.
    {
        let expected = vector![10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22];
        assert_eq!(*ob, expected);

        Observable::set(&mut limit, 0);
        assert_next_eq!(sub, VectorDiff::Clear);

        Observable::set(&mut limit, 42);
        assert_next_eq!(sub, VectorDiff::Append { values: expected });
    }

    assert_pending!(sub);
    drop(ob);
    assert_closed!(sub);
}

#[test]
fn clear() {
    let mut ob = ObservableVector::<usize>::new();
    let mut limit = Observable::new(0);
    let mut sub = ob.subscribe().dynamic_rlimit(Observable::subscribe(&limit));

    // Set limit to 4.
    Observable::set(&mut limit, 4);

    // Add 1 value.
    ob.push_back(10);

    // Clear.
    ob.clear();

    // Observe the updates.
    assert_next_eq!(sub, VectorDiff::Append { values: vector![10] });
    assert_next_eq!(sub, VectorDiff::Clear);

    // Check the content of the vector.
    {
        let expected = vector![];
        assert_eq!(*ob, expected);
    }

    drop(ob);
    assert_closed!(sub);
}

#[test]
fn push_front() {
    let mut ob = ObservableVector::<usize>::new();
    let mut limit = Observable::new(0);
    let mut sub = ob.subscribe().dynamic_rlimit(Observable::subscribe(&limit));

    // Set limit to 2.
    Observable::set(&mut limit, 2);

    // Force the limit to be polled first.
    assert_pending!(sub);

    // Vector is empty and limit is polled for the first time.
    {
        ob.push_front(10);

        assert_next_eq!(sub, VectorDiff::PushFront { value: 10 });

        // State of:
        //
        // - the vector: [ 10 ]
        // - the “view”: [ 10 ]
    }

    // Push front, the limit is not reached yet.
    {
        ob.push_front(11);

        assert_next_eq!(sub, VectorDiff::PushFront { value: 11 });

        // State of:
        //
        // - the vector: [ 11, 10 ]
        // - the “view”: [ 11, 10 ]
    }

    // Push front, it happens outside the limit, nothing happens.
    {
        ob.push_front(12);

        assert_pending!(sub);

        // State of:
        //
        // - the vector: [ 12, 11, 10 ]
        // - the “view”: [ 11, 10 ]
    }

    // Check the content of the vector.
    {
        let expected = vector![12, 11, 10];
        assert_eq!(*ob, expected);

        Observable::set(&mut limit, 0);
        assert_next_eq!(sub, VectorDiff::Clear);

        Observable::set(&mut limit, 42);
        assert_next_eq!(sub, VectorDiff::Append { values: expected });
    }

    assert_pending!(sub);
    drop(ob);
    assert_closed!(sub);
}

#[test]
fn push_back() {
    let mut ob = ObservableVector::<usize>::new();
    let mut limit = Observable::new(0);
    let mut sub = ob.subscribe().dynamic_rlimit(Observable::subscribe(&limit));

    // Set limit to 2.
    Observable::set(&mut limit, 2);

    // Force the limit to be polled first.
    assert_pending!(sub);

    // Vector is empty and limit is polled for the first time.
    {
        ob.push_back(10);

        assert_next_eq!(sub, VectorDiff::PushBack { value: 10 });

        // State of:
        //
        // - the vector: [ 10 ]
        // - the “view”: [ 10 ]
    }

    // Push back, the limit is not reached yet.
    {
        ob.push_back(11);

        assert_next_eq!(sub, VectorDiff::PushBack { value: 11 });

        // State of:
        //
        // - the vector: [ 10, 11 ]
        // - the “view”: [ 10, 11 ]
    }

    // Push back, the view is full, a value needs to be removed.
    {
        ob.push_back(12);

        assert_next_eq!(sub, VectorDiff::PopFront);
        assert_next_eq!(sub, VectorDiff::PushBack { value: 12 });

        // State of:
        //
        // - the vector: [ 10, 11, 12 ]
        // - the “view”: [ 10, 12 ]
    }

    // Check the content of the vector.
    {
        let expected = vector![10, 11, 12];
        assert_eq!(*ob, expected);

        Observable::set(&mut limit, 0);
        assert_next_eq!(sub, VectorDiff::Clear);

        Observable::set(&mut limit, 42);
        assert_next_eq!(sub, VectorDiff::Append { values: expected });
    }

    assert_pending!(sub);
    drop(ob);
    assert_closed!(sub);
}

#[test]
fn pop_front() {
    let mut ob = ObservableVector::<usize>::new();
    let mut limit = Observable::new(0);
    let mut sub = ob.subscribe().dynamic_rlimit(Observable::subscribe(&limit));

    // Init state.
    {
        ob.append(vector![10, 11, 12]);
        Observable::set(&mut limit, 2);

        assert_next_eq!(sub, VectorDiff::Append { values: vector![11, 12] });

        // State of:
        //
        // - the vector: [ 10, 11, 12 ]
        // - the “view”: [ 11, 12 ]
    }

    // Pop front, it happens outside the view, nothing happens.
    {
        ob.pop_front(); // 10

        assert_pending!(sub);

        // State of:
        //
        // - the vector: [ 11, 12 ]
        // - the “view”: [ 11, 12 ]
    }

    // Pop front, it happens inside the view.
    {
        ob.pop_front(); // 11

        assert_next_eq!(sub, VectorDiff::PopFront);

        // State of:
        //
        // - the vector: [ 12 ]
        // - the “view”: [ 12 ]
    }

    // Check the content of the vector.
    {
        let expected = vector![12];
        assert_eq!(*ob, expected);

        Observable::set(&mut limit, 0);
        assert_next_eq!(sub, VectorDiff::Clear);

        Observable::set(&mut limit, 42);
        assert_next_eq!(sub, VectorDiff::Append { values: expected });
    }

    assert_pending!(sub);
    drop(ob);
    assert_closed!(sub);
}

#[test]
fn pop_back() {
    let mut ob = ObservableVector::<usize>::new();
    let mut limit = Observable::new(0);
    let mut sub = ob.subscribe().dynamic_rlimit(Observable::subscribe(&limit));

    // Init state.
    {
        ob.append(vector![10, 11, 12]);
        Observable::set(&mut limit, 2);

        assert_next_eq!(sub, VectorDiff::Append { values: vector![11, 12] });

        // State of:
        //
        // - the vector: [ 10, 11, 12 ]
        // - the “view”: [ 11, 12 ]
    }

    // Pop back, the buffer has a value to fill the view.
    {
        ob.pop_back(); // 12

        assert_next_eq!(sub, VectorDiff::PopBack);
        assert_next_eq!(sub, VectorDiff::PushFront { value: 10 });

        // State of:
        //
        // - the vector: [ 10, 11 ]
        // - the “view”: [ 10, 11 ]
    }

    // Pop back, the buffer has no value to fill the view.
    {
        ob.pop_back(); // 11

        assert_next_eq!(sub, VectorDiff::PopBack);

        // State of:
        //
        // - the vector: [ 10 ]
        // - the “view”: [ 10 ]
    }

    // Check the content of the vector.
    {
        let expected = vector![10];
        assert_eq!(*ob, expected);

        Observable::set(&mut limit, 0);
        assert_next_eq!(sub, VectorDiff::Clear);

        Observable::set(&mut limit, 42);
        assert_next_eq!(sub, VectorDiff::Append { values: expected });
    }

    assert_pending!(sub);
    drop(ob);
    assert_closed!(sub);
}

#[test]
fn insert() {
    let mut ob = ObservableVector::<usize>::new();
    let mut limit = Observable::new(0);
    let mut sub = ob.subscribe().dynamic_rlimit(Observable::subscribe(&limit));

    Observable::set(&mut limit, 2);

    // Force the limit to be polled first.
    assert_pending!(sub);

    // Insert at 0, on an empty vector.
    {
        ob.insert(0, 10);

        assert_next_eq!(sub, VectorDiff::Insert { index: 0, value: 10 });

        // State of:
        //
        // - the vector: [ 10 ]
        // - the “view”: [ 10 ]
    }

    // Insert at 0, on a non-empty vector, still inside the view.
    {
        ob.insert(0, 11);

        assert_next_eq!(sub, VectorDiff::Insert { index: 0, value: 11 });

        // State of:
        //
        // - the vector: [ 11, 10 ]
        // - the “view”: [ 11, 10 ]
    }

    // Insert at 0, on a non-empty vector, but this time, outside the view.
    {
        ob.insert(0, 12);

        assert_pending!(sub);

        // State of:
        //
        // - the vector: [ 12, 11, 10 ]
        // - the “view”: [ 11, 10 ]
    }

    // Insert outside the view, nothing happens.
    {
        ob.insert(1, 13);

        assert_pending!(sub);

        // State of:
        //
        // - the vector: [ 12, 13, 11, 10 ]
        // - the “view”: [ 11, 10 ]
    }

    // Insert outside the view (exactly at the limit, to test off-by-one), nothing
    // happens.
    {
        ob.insert(2, 14);

        assert_pending!(sub);

        // State of:
        //
        // - the vector: [ 12, 13, 14, 11, 10 ]
        // - the “view”: [ 11, 10 ]
    }

    // Insert inside the view.
    {
        ob.insert(4, 15);

        assert_next_eq!(sub, VectorDiff::PopFront);
        assert_next_eq!(sub, VectorDiff::Insert { index: 0, value: 15 });

        // State of:
        //
        // - the vector: [ 12, 13, 14, 11, 15, 10 ]
        // - the “view”: [ 15, 10 ]
    }

    // Insert inside a larger view.
    {
        Observable::set(&mut limit, 4);

        assert_next_eq!(sub, VectorDiff::PushFront { value: 11 });
        assert_next_eq!(sub, VectorDiff::PushFront { value: 14 });

        // State of:
        //
        // - the vector: [ 12, 13, 14, 11, 15, 10 ]
        // - the “view”: [ 14, 11, 15, 10 ]

        ob.insert(4, 16);

        assert_next_eq!(sub, VectorDiff::PopFront); // 14
        assert_next_eq!(sub, VectorDiff::Insert { index: 1, value: 16 });

        // State of:
        //
        // - the vector: [ 12, 13, 14, 16, 11, 15, 10 ]
        // - the “view”: [ 11, 16, 15, 10 ]
    }

    // Check the content of the vector.
    {
        let expected = vector![12, 13, 14, 11, 16, 15, 10];
        assert_eq!(*ob, expected);

        Observable::set(&mut limit, 0);
        assert_next_eq!(sub, VectorDiff::Clear);

        Observable::set(&mut limit, 42);
        assert_next_eq!(sub, VectorDiff::Append { values: expected });
    }

    assert_pending!(sub);
    drop(ob);
    assert_closed!(sub);
}

#[test]
fn set() {
    let mut ob = ObservableVector::<usize>::new();
    let mut limit = Observable::new(0);
    let mut sub = ob.subscribe().dynamic_rlimit(Observable::subscribe(&limit));

    // Init state.
    {
        ob.append(vector![10, 11, 12]);
        Observable::set(&mut limit, 2);

        assert_next_eq!(sub, VectorDiff::Append { values: vector![11, 12] });

        // State of:
        //
        // - the vector: [ 10, 11, 12 ]
        // - the “view”: [ 11, 12 ]
    }

    // Set outside the view, nothing happens.
    {
        ob.set(0, 20);

        assert_pending!(sub);

        // State of:
        //
        // - the vector: [ 20, 11, 12 ]
        // - the “view”: [ 11, 12 ]
    }

    // Set inside the view.
    {
        ob.set(1, 21);

        assert_next_eq!(sub, VectorDiff::Set { index: 0, value: 21 });

        // State of:
        //
        // - the vector: [ 20, 21, 12 ]
        // - the “view”: [ 21, 12 ]
    }

    // Set inside a larger view.
    {
        Observable::set(&mut limit, 4);

        assert_next_eq!(sub, VectorDiff::PushFront { value: 20 });

        // State of:
        //
        // - the vector: [ 20, 21, 12 ]
        // - the “view”: [ _, 20, 21, 12 ]

        ob.append(vector![13, 14, 15]);

        assert_next_eq!(sub, VectorDiff::PopFront);
        assert_next_eq!(sub, VectorDiff::PopFront);
        assert_next_eq!(sub, VectorDiff::Append { values: vector![13, 14, 15] });

        // State of:
        //
        // - the vector: [ 20, 21, 12, 13, 14, 15 ]
        // - the “view”: [ 12, 13, 14, 15 ]

        ob.set(4, 24);

        assert_next_eq!(sub, VectorDiff::Set { index: 2, value: 24 });

        // State of:
        //
        // - the vector: [ 20, 21, 12, 13, 24, 15 ]
        // - the “view”: [ 12, 13, 24, 15 ]
    }

    // Check the content of the vector.
    {
        let expected = vector![20, 21, 12, 13, 24, 15];
        assert_eq!(*ob, expected);

        Observable::set(&mut limit, 0);
        assert_next_eq!(sub, VectorDiff::Clear);

        Observable::set(&mut limit, 42);
        assert_next_eq!(sub, VectorDiff::Append { values: expected });
    }

    assert_pending!(sub);
    drop(ob);
    assert_closed!(sub);
}

#[test]
fn remove() {
    let mut ob = ObservableVector::<usize>::new();
    let mut limit = Observable::new(0);
    let mut sub = ob.subscribe().dynamic_rlimit(Observable::subscribe(&limit));

    // Init state.
    {
        ob.append(vector![10, 11, 12, 13, 14, 15]);
        Observable::set(&mut limit, 4);

        assert_next_eq!(sub, VectorDiff::Append { values: vector![12, 13, 14, 15] });

        // State of:
        //
        // - the vector: [ 10, 11, 12, 13, 14, 15 ]
        // - the “view”: [ 12, 13, 14, 15 ]
    }

    // Remove outside the limit.
    {
        ob.remove(1);

        assert_pending!(sub);

        // State of:
        //
        // - the vector: [ 10, 12, 13, 14, 15 ]
        // - the “view”: [ 12, 13, 14, 15 ]
    }

    // Remove inside the limit, the buffer has a value to fill the view.
    {
        ob.remove(1);

        assert_next_eq!(sub, VectorDiff::Remove { index: 0 }); // 12
        assert_next_eq!(sub, VectorDiff::PushFront { value: 10 });

        // State of:
        //
        // - the vector: [ 10, 13, 14, 15 ]
        // - the “view”: [ 10, 13, 14, 15 ]
    }

    // Remove inside the limit, the buffer has no value to fill the view.
    {
        ob.remove(1);

        assert_next_eq!(sub, VectorDiff::Remove { index: 1 }); // 13
        assert_pending!(sub); // and that's it!

        // State of:
        //
        // - the vector: [ 10, 14, 15 ]
        // - the “view”: [ 10, 14, 15 ]
    }

    // Remove inside the limit, the buffer has no value to fill the view, and the
    // limit is larger than the buffer.
    {
        ob.remove(1);

        assert_next_eq!(sub, VectorDiff::Remove { index: 1 }); // 14
        assert_pending!(sub); // and that's it!

        // State of:
        //
        // - the vector: [ 10, 15 ]
        // - the “view”: [ 10, 15 ]
    }

    // Remove again.
    {
        ob.remove(0);

        assert_next_eq!(sub, VectorDiff::Remove { index: 0 }); // 10

        // State of:
        //
        // - the vector: [ 15 ]
        // - the “view”: [ 15 ]
    }

    // Check the content of the vector.
    {
        let expected = vector![15];
        assert_eq!(*ob, expected);

        Observable::set(&mut limit, 0);
        assert_next_eq!(sub, VectorDiff::Clear);

        Observable::set(&mut limit, 42);
        assert_next_eq!(sub, VectorDiff::Append { values: expected });
    }

    assert_pending!(sub);
    drop(ob);
    assert_closed!(sub);
}

#[test]
fn truncate() {
    let mut ob = ObservableVector::<usize>::new();
    let mut limit = Observable::new(0);
    let mut sub = ob.subscribe().dynamic_rlimit(Observable::subscribe(&limit));

    // Truncate to a size that compares to the limit as follows:
    //
    // | compares to the limit | values to fill in | test case  |
    // |-----------------------|-------------------|------------|
    // | larger                | yes               | #1         |
    // | equal                 | yes               | #2         |
    // | smaller               | yes               | #3         |
    // | larger                | no                | impossible |
    // | equal                 | no                | impossible |
    // | smaller               | no                | #4         |
    //
    // Why is it _impossible_ to truncate to a size larger or equal to the limit,
    // with no buffer values to fill in? To get no buffer values available to fill
    // in the view, we need the limit to be greater than or equal to the size of the
    // vector. As such, the truncate size must be larger than the size of the
    // vector, which is a no-op: reading the documentation of
    // `ObservableVector::truncate`, it tells it does nothing.

    // Init state.
    {
        ob.append(vector![10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21]);
        Observable::set(&mut limit, 4);

        assert_next_eq!(sub, VectorDiff::Append { values: vector![18, 19, 20, 21] });

        // State of:
        //
        // - the vector: [ 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21 ]
        // - the “view”: [ 18, 19, 20, 21 ]
    }

    // Test case #1.
    //
    // Truncate to a size that is larger than the limit, the buffer has values to
    // fill the view.
    {
        ob.truncate(10);

        assert_next_eq!(sub, VectorDiff::PopBack); // 21
        assert_next_eq!(sub, VectorDiff::PopBack); // 20
        assert_next_eq!(sub, VectorDiff::PushFront { value: 17 });
        assert_next_eq!(sub, VectorDiff::PushFront { value: 16 });

        // State of:
        //
        // - the vector: [ 10, 11, 12, 13, 14, 15, 16, 17, 18, 19 ]
        // - the “view”: [ 16, 17, 18, 19 ]
    }

    // Test case #2.
    //
    // Truncate to a size that is equal to the limit, the buffer has values to fill
    // the view.
    {
        Observable::set(&mut limit, 8);

        assert_next_eq!(sub, VectorDiff::PushFront { value: 15 });
        assert_next_eq!(sub, VectorDiff::PushFront { value: 14 });
        assert_next_eq!(sub, VectorDiff::PushFront { value: 13 });
        assert_next_eq!(sub, VectorDiff::PushFront { value: 12 });

        // State of:
        //
        // - the vector: [ 10, 11, 12, 13, 14, 15, 16, 17, 18, 19 ]
        // - the “view”: [ 12, 13, 14, 15, 16, 17, 18, 19 ]

        ob.truncate(8);

        assert_next_eq!(sub, VectorDiff::PopBack); // 19
        assert_next_eq!(sub, VectorDiff::PopBack); // 18
        assert_next_eq!(sub, VectorDiff::PushFront { value: 11 });
        assert_next_eq!(sub, VectorDiff::PushFront { value: 10 });

        // State of:
        //
        // - the vector: [ 10, 11, 12, 13, 14, 15, 16, 17 ]
        // - the “view”: [ 10, 11, 12, 13, 14, 15, 16, 17 ]
    }

    // Test case #3.
    //
    // Truncate to a size that is lower to the limit, the buffer has values to fill
    // the view.
    {
        Observable::set(&mut limit, 7);

        assert_next_eq!(sub, VectorDiff::PopFront);

        // State of:
        //
        // - the vector: [ 10, 11, 12, 13, 14, 15, 16, 17 ]
        // - the “view”: [ 11, 12, 13, 14, 15, 16, 17 ]

        ob.truncate(6);

        assert_next_eq!(sub, VectorDiff::PopBack); // 17
        assert_next_eq!(sub, VectorDiff::PopBack); // 16
        assert_next_eq!(sub, VectorDiff::PushFront { value: 10 });

        // State of:
        //
        // - the vector: [ 10, 11, 12, 13, 14, 15 ]
        // - the “view”: [ 10, 11, 12, 13, 14, 15 ]
    }

    // Test case #4.
    //
    // Truncate to a size that is smaller than the limit, the buffer has no values
    // to fill the view.
    {
        Observable::set(&mut limit, 8);

        assert_pending!(sub);

        // State of:
        //
        // - the vector: [ 10, 11, 12, 13, 14, 15 ]
        // - the “view”: [ _, _, 10, 11, 12, 13, 14, 15 ]

        ob.truncate(3);

        assert_next_eq!(sub, VectorDiff::PopBack); // 15
        assert_next_eq!(sub, VectorDiff::PopBack); // 14
        assert_next_eq!(sub, VectorDiff::PopBack); // 13
        assert_pending!(sub); // that's it!

        // State of:
        //
        // - the vector: [ 10, 11, 12 ]
        // - the “view”: [ _, _, _, _, _, 10, 11, 12 ]
    }

    // Check the content of the vector.
    {
        let expected = vector![10, 11, 12];
        assert_eq!(*ob, expected);

        Observable::set(&mut limit, 0);
        assert_next_eq!(sub, VectorDiff::Clear);

        Observable::set(&mut limit, 42);
        assert_next_eq!(sub, VectorDiff::Append { values: expected });
    }

    assert_pending!(sub);
    drop(ob);
    assert_closed!(sub);
}

#[test]
fn reset() {
    let mut ob = ObservableVector::<usize>::with_capacity(2);
    let mut limit = Observable::new(0);
    let mut sub = ob.subscribe().dynamic_rlimit(Observable::subscribe(&limit));

    // Init state.
    {
        ob.append(vector![10, 11, 12, 13, 14, 15]);
        Observable::set(&mut limit, 4);

        assert_next_eq!(sub, VectorDiff::Append { values: vector![12, 13, 14, 15] });

        // State of:
        //
        // - the vector: [ 10, 11, 12, 13, 14, 15 ]
        // - the “view”: [ 12, 13, 14, 15 ]
    }

    // Do multiple operations to saturate the observable capacity, in order to
    // trigger a reset.
    {
        ob.push_back(16);
        ob.push_back(17);
        ob.push_back(18);
    }

    // Observe a reset, capped to the limit.
    {
        assert_next_eq!(sub, VectorDiff::Reset { values: vector![15, 16, 17, 18] });

        // State of:
        //
        // - the vector: [ 10, 11, 12, 13, 14, 15, 16, 17, 18 ]
        // - the “view”: [ 15, 16, 17, 18 ]
    }

    // Check the content of the vector.
    {
        let expected = vector![10, 11, 12, 13, 14, 15, 16, 17, 18];
        assert_eq!(*ob, expected);

        Observable::set(&mut limit, 0);
        assert_next_eq!(sub, VectorDiff::Clear);

        Observable::set(&mut limit, 42);
        assert_next_eq!(sub, VectorDiff::Append { values: expected });
    }

    assert_pending!(sub);
    drop(ob);
    assert_closed!(sub);
}

// This test is copied (and modified to match the behaviour of `RLimit`) from
// `Limit`'s test suite. The bug is not present in `RLimit`, but since their
// behaviours are quite similar, let's ensure the bug cannot happen
// preemptively.
#[tokio::test]
async fn limit_stream_wake() {
    use futures_util::{FutureExt, StreamExt};
    use tokio::task::yield_now;

    let ob = ObservableVector::<u32>::from(vector![1, 2]);
    let mut limit = Observable::new(1);
    let (values, mut sub) =
        ob.subscribe().dynamic_rlimit_with_initial_value(1, Observable::subscribe(&limit));

    assert_eq!(values, vector![2]);

    let task_hdl = tokio::spawn(async move {
        let update = sub.next().await.unwrap();
        assert_eq!(update, VectorDiff::PushFront { value: 1 });
    });

    // Set the limit to the same value that `RLimit` has already seen.
    Observable::set(&mut limit, 1);

    // Make sure the task spawned above sees the "updated" limit.
    yield_now().await;

    // Now update the limit again.
    Observable::set(&mut limit, 2);

    // Allow the task to make progress.
    yield_now().await;

    // It should be finished now.
    task_hdl.now_or_never().unwrap().unwrap();
}

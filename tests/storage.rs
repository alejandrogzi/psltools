// Copyright (c) 2026 Alejandro Gonzales-Irribarren <alejandrxgzi@gmail.com>
// Distributed under the terms of the Apache License, Version 2.0.

use psltools::{ByteSlice, SharedBytes};

#[test]
fn byte_slice_views_and_utf8() {
    let storage = SharedBytes::from_owned(b"hello world".to_vec());
    let slice = ByteSlice::new(storage, 6..11);
    assert_eq!(slice.as_bytes(), b"world");
    assert_eq!(slice.as_str(), Some("world"));
    assert_eq!(slice.len(), 5);
    assert!(!slice.is_empty());
}

#[test]
fn byte_slice_invalid_utf8_is_none() {
    let storage = SharedBytes::from_owned(vec![0xff, 0xfe]);
    let slice = ByteSlice::new(storage, 0..2);
    assert_eq!(slice.as_str(), None);
}

#[test]
fn cloning_byte_slice_is_cheap_and_consistent() {
    let storage = SharedBytes::from_owned(b"abcdef".to_vec());
    let a = ByteSlice::new(storage, 1..4);
    let b = a.clone();
    assert_eq!(a.as_bytes(), b.as_bytes());
    assert_eq!(b.as_bytes(), b"bcd");
}

use std::str::{self, Utf8Error};

use crate::collections::SVec;
use crate::OutOfMemory;

pub enum SStringError {
    FromUtf8Error((Vec<u8>, Utf8Error)),
    OutOfMemory,
}

pub struct SString {
    inner: SVec<u8>,
}

impl SString {
    pub fn new() -> Self {
        Self { inner: SVec::new() }
    }

    pub fn new_with_capacity(capacity: usize) -> Result<Self, OutOfMemory> {
        Ok(Self {
            inner: SVec::new_with_capacity(capacity)?,
        })
    }

    pub fn from_utf8(vec: Vec<u8>) -> Result<SString, SStringError> {
        match str::from_utf8(&vec) {
            Ok(..) => SVec::<u8>::from_std_blob(vec)
                .map(|inner| SString { inner })
                .map_err(|_| SStringError::OutOfMemory),
            Err(e) => Err(SStringError::FromUtf8Error((vec, e))),
        }
    }

    pub unsafe fn from_utf8_unchecked(bytes: Vec<u8>) -> Result<SString, OutOfMemory> {
        Ok(SString {
            inner: SVec::<u8>::from_std_blob(bytes)?,
        })
    }

    pub fn into_bytes(self) -> SVec<u8> {
        self.inner
    }

    pub fn as_bytes(&self) -> &SVec<u8> {
        &self.inner
    }
}

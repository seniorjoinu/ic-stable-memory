use crate::types::{Word, PAGE_SIZE_BYTES};
use ic_cdk::api::stable::{stable64_grow, stable64_read, stable64_size, stable64_write};

pub struct OutOfMemory;

pub trait MemContext {
    fn size_pages(&self) -> u64;
    fn grow(&mut self, new_pages: u64) -> Result<(), OutOfMemory>;
    fn read(&self, offset: Word, buf: &mut [u8]);
    fn write(&mut self, offset: Word, buf: &[u8]);

    fn offset_exists(&self, offset: Word) -> bool {
        self.size_pages() * PAGE_SIZE_BYTES as Word >= offset
    }
}

#[derive(Clone)]
pub struct StableMemContext;

impl MemContext for StableMemContext {
    fn size_pages(&self) -> u64 {
        stable64_size()
    }

    fn grow(&mut self, new_pages: u64) -> Result<(), OutOfMemory> {
        stable64_grow(new_pages)
            .map_err(|_| OutOfMemory)
            .map(|_| ())
    }

    fn read(&self, offset: Word, buf: &mut [u8]) {
        stable64_read(offset, buf)
    }

    fn write(&mut self, offset: Word, buf: &[u8]) {
        stable64_write(offset, buf)
    }
}

#[derive(Default, Clone)]
pub struct TestMemContext {
    pub data: Vec<u8>,
}

impl MemContext for TestMemContext {
    fn size_pages(&self) -> u64 {
        self.data.len() as u64 / PAGE_SIZE_BYTES as Word
    }

    fn grow(&mut self, new_pages: u64) -> Result<(), OutOfMemory> {
        self.data
            .extend(vec![0; PAGE_SIZE_BYTES * new_pages as usize]);

        Ok(())
    }

    fn read(&self, offset: Word, buf: &mut [u8]) {
        for i in offset..offset + buf.len() as Word {
            buf[(i - offset) as usize] = *self.data.get(i as usize).unwrap()
        }
    }

    fn write(&mut self, offset: Word, buf: &[u8]) {
        for i in offset..offset + buf.len() as Word {
            std::mem::replace(&mut self.data[i as usize], buf[(i - offset) as usize]);
        }
    }
}
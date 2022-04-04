use crate::types::PAGE_SIZE_BYTES;
use ic_cdk::api::stable::{stable64_grow, stable64_read, stable64_size, stable64_write};

#[derive(Debug, Copy, Clone)]
pub struct OutOfMemory;

pub trait MemContext {
    fn size_pages(&self) -> u64;
    fn grow(&mut self, new_pages: u64) -> Result<(), OutOfMemory>;
    fn read(&self, offset: u64, buf: &mut [u8]);
    fn write(&mut self, offset: u64, buf: &[u8]);

    fn offset_exists(&self, offset: u64) -> bool {
        self.size_pages() * PAGE_SIZE_BYTES as u64 >= offset
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

    fn read(&self, offset: u64, buf: &mut [u8]) {
        stable64_read(offset, buf)
    }

    fn write(&mut self, offset: u64, buf: &[u8]) {
        stable64_write(offset, buf)
    }
}

#[derive(Default, Clone)]
pub struct TestMemContext {
    pub data: Vec<u8>,
}

impl MemContext for TestMemContext {
    fn size_pages(&self) -> u64 {
        self.data.len() as u64 / PAGE_SIZE_BYTES as u64
    }

    fn grow(&mut self, new_pages: u64) -> Result<(), OutOfMemory> {
        self.data
            .extend(vec![0; PAGE_SIZE_BYTES * new_pages as usize]);

        Ok(())
    }

    fn read(&self, offset: u64, buf: &mut [u8]) {
        for i in offset..offset + buf.len() as u64 {
            buf[(i - offset) as usize] = self.data[i as usize]
        }
    }

    fn write(&mut self, offset: u64, buf: &[u8]) {
        self.data.splice(
            (offset as usize)..(offset as usize + buf.len()),
            Vec::from(buf),
        );
    }
}

#[cfg(target_family = "wasm")]
pub mod stable {
    use crate::mem_context::{OutOfMemory, StableMemContext};

    pub fn size_pages() -> u64 {
        StableMemContext::size_pages()
    }

    pub fn grow(new_pages: u64) -> Result<(), OutOfMemory> {
        StableMemContext::grow(new_pages)
    }

    pub fn read(offset: u64, buf: &mut [u8]) {
        StableMemContext::read(offset, buf)
    }

    pub fn write(offset: u64, buf: &[u8]) {
        StableMemContext::write(offset, buf)
    }
}

#[cfg(not(target_family = "wasm"))]
pub mod stable {
    use crate::mem_context::{MemContext, OutOfMemory, TestMemContext};

    static mut CONTEXT: TestMemContext = TestMemContext { data: vec![] };

    pub fn clear() {
        unsafe {
            CONTEXT = TestMemContext { data: vec![] };
        }
    }

    pub fn _get_context() -> &'static mut TestMemContext {
        unsafe { &mut CONTEXT }
    }

    pub fn size_pages() -> u64 {
        unsafe { CONTEXT.size_pages() }
    }

    pub fn grow(new_pages: u64) -> Result<(), OutOfMemory> {
        unsafe { CONTEXT.grow(new_pages) }
    }

    pub fn read(offset: u64, buf: &mut [u8]) {
        unsafe { CONTEXT.read(offset, buf) }
    }

    pub fn write(offset: u64, buf: &[u8]) {
        unsafe { CONTEXT.write(offset, buf) }
    }
}

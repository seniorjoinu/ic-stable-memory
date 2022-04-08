use ic_cdk::api::stable::{stable64_grow, stable64_read, stable64_size, stable64_write};

pub const PAGE_SIZE_BYTES: usize = 64 * 1024;

#[derive(Debug, Copy, Clone)]
pub struct OutOfMemory;

pub(crate) trait MemContext {
    fn size_pages(&self) -> u64;
    fn grow(&mut self, new_pages: u64) -> Result<u64, OutOfMemory>;
    fn read(&self, offset: u64, buf: &mut [u8]);
    fn write(&mut self, offset: u64, buf: &[u8]);

    fn offset_exists(&self, offset: u64) -> bool {
        self.size_pages() * PAGE_SIZE_BYTES as u64 >= offset
    }
}

#[derive(Clone)]
pub(crate) struct StableMemContext;

impl MemContext for StableMemContext {
    fn size_pages(&self) -> u64 {
        stable64_size()
    }

    fn grow(&mut self, new_pages: u64) -> Result<u64, OutOfMemory> {
        stable64_grow(new_pages).map_err(|_| OutOfMemory)
    }

    fn read(&self, offset: u64, buf: &mut [u8]) {
        stable64_read(offset, buf)
    }

    fn write(&mut self, offset: u64, buf: &[u8]) {
        stable64_write(offset, buf)
    }
}

#[derive(Default, Clone)]
pub(crate) struct TestMemContext {
    pub data: Vec<u8>,
}

impl MemContext for TestMemContext {
    fn size_pages(&self) -> u64 {
        self.data.len() as u64 / PAGE_SIZE_BYTES as u64
    }

    fn grow(&mut self, new_pages: u64) -> Result<u64, OutOfMemory> {
        let prev_pages = self.size_pages();
        self.data
            .extend(vec![0; PAGE_SIZE_BYTES * new_pages as usize]);

        Ok(prev_pages)
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
    use crate::utils::mem_context::{MemContext, OutOfMemory, StableMemContext};

    pub fn size_pages() -> u64 {
        MemContext::size_pages(&StableMemContext)
    }

    pub fn grow(new_pages: u64) -> Result<u64, OutOfMemory> {
        MemContext::grow(&mut StableMemContext, new_pages)
    }

    pub fn read(offset: u64, buf: &mut [u8]) {
        MemContext::read(&StableMemContext, offset, buf)
    }

    pub fn write(offset: u64, buf: &[u8]) {
        MemContext::write(&mut StableMemContext, offset, buf)
    }
}

#[cfg(not(target_family = "wasm"))]
pub mod stable {
    use crate::utils::mem_context::{MemContext, OutOfMemory, TestMemContext};

    static mut CONTEXT: TestMemContext = TestMemContext { data: vec![] };

    pub fn clear() {
        unsafe {
            CONTEXT = TestMemContext { data: vec![] };
        }
    }

    pub(crate) fn _get_context() -> &'static mut TestMemContext {
        unsafe { &mut CONTEXT }
    }

    pub fn size_pages() -> u64 {
        unsafe { CONTEXT.size_pages() }
    }

    pub fn grow(new_pages: u64) -> Result<u64, OutOfMemory> {
        unsafe { CONTEXT.grow(new_pages) }
    }

    pub fn read(offset: u64, buf: &mut [u8]) {
        unsafe { CONTEXT.read(offset, buf) }
    }

    pub fn write(offset: u64, buf: &[u8]) {
        unsafe { CONTEXT.write(offset, buf) }
    }
}

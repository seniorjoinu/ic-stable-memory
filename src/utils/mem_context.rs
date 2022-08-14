use ic_cdk::api::stable::{stable64_grow, stable64_read, stable64_size, stable64_write};
use std::cmp::min;

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
    pub pages: Vec<[u8; PAGE_SIZE_BYTES]>,
}

impl MemContext for TestMemContext {
    fn size_pages(&self) -> u64 {
        self.pages.len() as u64
    }

    fn grow(&mut self, new_pages: u64) -> Result<u64, OutOfMemory> {
        let prev_pages = self.size_pages();

        for _ in 0..new_pages {
            self.pages.push([0u8; PAGE_SIZE_BYTES]);
        }

        Ok(prev_pages)
    }

    fn read(&self, offset: u64, buf: &mut [u8]) {
        let start_page_idx = (offset / PAGE_SIZE_BYTES as u64) as usize;
        let start_page_inner_idx = (offset % PAGE_SIZE_BYTES as u64) as usize;
        let start_page_size = min(PAGE_SIZE_BYTES - start_page_inner_idx, buf.len());

        let (pages_in_between, last_page_size) = if start_page_size == buf.len() {
            (0usize, 0usize)
        } else {
            (
                (buf.len() - start_page_size) / PAGE_SIZE_BYTES,
                (buf.len() - start_page_size) % PAGE_SIZE_BYTES,
            )
        };

        // read first page
        buf[0..start_page_size].copy_from_slice(
            &self.pages[start_page_idx]
                [start_page_inner_idx..(start_page_inner_idx + start_page_size)],
        );

        // read pages in-between
        for i in 0..pages_in_between {
            buf[(start_page_size + i * PAGE_SIZE_BYTES)
                ..(start_page_size + (i + 1) * PAGE_SIZE_BYTES)]
                .copy_from_slice(&self.pages[start_page_idx + i + 1]);
        }

        // read last pages
        if last_page_size == 0 {
            return;
        }

        buf[(start_page_size + pages_in_between * PAGE_SIZE_BYTES)
            ..(start_page_size + pages_in_between * PAGE_SIZE_BYTES + last_page_size)]
            .copy_from_slice(&self.pages[start_page_idx + pages_in_between + 1][0..last_page_size]);
    }

    fn write(&mut self, offset: u64, buf: &[u8]) {
        let start_page_idx = (offset / PAGE_SIZE_BYTES as u64) as usize;
        let start_page_inner_idx = (offset % PAGE_SIZE_BYTES as u64) as usize;
        let start_page_size = min(PAGE_SIZE_BYTES - start_page_inner_idx, buf.len());

        let (pages_in_between, last_page_size) = if start_page_size == buf.len() {
            (0usize, 0usize)
        } else {
            (
                (buf.len() - start_page_size) / PAGE_SIZE_BYTES,
                (buf.len() - start_page_size) % PAGE_SIZE_BYTES,
            )
        };

        // write to first page
        self.pages[start_page_idx][start_page_inner_idx..(start_page_inner_idx + start_page_size)]
            .copy_from_slice(&buf[0..start_page_size]);

        // write to pages in-between
        for i in 0..pages_in_between {
            self.pages[start_page_idx + i + 1].copy_from_slice(
                &buf[(start_page_size + i * PAGE_SIZE_BYTES)
                    ..(start_page_size + (i + 1) * PAGE_SIZE_BYTES)],
            );
        }

        // write to last page
        if last_page_size == 0 {
            return;
        }

        self.pages[start_page_idx + pages_in_between + 1][0..last_page_size].copy_from_slice(
            &buf[(start_page_size + pages_in_between * PAGE_SIZE_BYTES)
                ..(start_page_size + pages_in_between * PAGE_SIZE_BYTES + last_page_size)],
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
    use std::cell::RefCell;

    thread_local! {
        static CONTEXT: RefCell<TestMemContext> = RefCell::new(TestMemContext::default());
    }

    pub fn clear() {
        CONTEXT.with(|it| it.borrow_mut().pages.clear())
    }

    pub fn size_pages() -> u64 {
        CONTEXT.with(|it| it.borrow().size_pages())
    }

    pub fn grow(new_pages: u64) -> Result<u64, OutOfMemory> {
        CONTEXT.with(|it| it.borrow_mut().grow(new_pages))
    }

    pub fn read(offset: u64, buf: &mut [u8]) {
        CONTEXT.with(|it| it.borrow().read(offset, buf))
    }

    pub fn write(offset: u64, buf: &[u8]) {
        CONTEXT.with(|it| it.borrow_mut().write(offset, buf))
    }
}

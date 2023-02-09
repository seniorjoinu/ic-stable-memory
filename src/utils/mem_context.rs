use std::cmp::min;

pub const PAGE_SIZE_BYTES: u64 = 64 * 1024;

#[derive(Debug, Copy, Clone)]
pub struct OutOfMemory;

pub(crate) trait MemContext {
    fn size_pages(&self) -> u64;
    fn grow(&mut self, new_pages: u64) -> Result<u64, OutOfMemory>;
    fn read(&self, offset: u64, buf: &mut [u8]);
    fn write(&mut self, offset: u64, buf: &[u8]);
}

#[derive(Clone)]
pub(crate) struct StableMemContext;

#[cfg(target_family = "wasm")]
use ic_cdk::api::stable::{stable64_grow, stable64_read, stable64_size, stable64_write};

#[cfg(target_family = "wasm")]
impl MemContext for StableMemContext {
    #[inline]
    fn size_pages(&self) -> u64 {
        stable64_size()
    }

    #[inline]
    fn grow(&mut self, new_pages: u64) -> Result<u64, OutOfMemory> {
        stable64_grow(new_pages).map_err(|_| OutOfMemory)
    }

    #[inline]
    fn read(&self, offset: u64, buf: &mut [u8]) {
        stable64_read(offset, buf)
    }

    #[inline]
    fn write(&mut self, offset: u64, buf: &[u8]) {
        stable64_write(offset, buf)
    }
}

#[derive(Clone)]
pub(crate) struct TestMemContext {
    pub pages: Vec<[u8; PAGE_SIZE_BYTES as usize]>,
}

impl TestMemContext {
    const fn default() -> Self {
        Self { pages: Vec::new() }
    }
}

impl MemContext for TestMemContext {
    #[inline]
    fn size_pages(&self) -> u64 {
        self.pages.len() as u64
    }

    fn grow(&mut self, new_pages: u64) -> Result<u64, OutOfMemory> {
        let prev_pages = self.size_pages();

        for _ in 0..new_pages {
            self.pages.push([0u8; PAGE_SIZE_BYTES as usize]);
        }

        Ok(prev_pages)
    }

    fn read(&self, offset: u64, buf: &mut [u8]) {
        let start_page_idx = (offset / PAGE_SIZE_BYTES as u64) as usize;
        let start_page_inner_idx = (offset % PAGE_SIZE_BYTES as u64) as usize;
        let start_page_size = min(PAGE_SIZE_BYTES as usize - start_page_inner_idx, buf.len());

        let (pages_in_between, last_page_size) = if start_page_size == buf.len() {
            (0usize, 0usize)
        } else {
            (
                (buf.len() - start_page_size) / PAGE_SIZE_BYTES as usize,
                (buf.len() - start_page_size) % PAGE_SIZE_BYTES as usize,
            )
        };

        // read first page
        buf[0..start_page_size].copy_from_slice(
            &self.pages[start_page_idx]
                [start_page_inner_idx..(start_page_inner_idx + start_page_size)],
        );

        // read pages in-between
        for i in 0..pages_in_between {
            buf[(start_page_size + i * PAGE_SIZE_BYTES as usize)
                ..(start_page_size + (i + 1) * PAGE_SIZE_BYTES as usize)]
                .copy_from_slice(&self.pages[start_page_idx + i + 1]);
        }

        // read last pages
        if last_page_size == 0 {
            return;
        }

        buf[(start_page_size + pages_in_between * PAGE_SIZE_BYTES as usize)
            ..(start_page_size + pages_in_between * PAGE_SIZE_BYTES as usize + last_page_size)]
            .copy_from_slice(&self.pages[start_page_idx + pages_in_between + 1][0..last_page_size]);
    }

    fn write(&mut self, offset: u64, buf: &[u8]) {
        let start_page_idx = (offset / PAGE_SIZE_BYTES) as usize;
        let start_page_inner_idx = (offset % PAGE_SIZE_BYTES) as usize;
        let start_page_size = min(PAGE_SIZE_BYTES as usize - start_page_inner_idx, buf.len());

        let (pages_in_between, last_page_size) = if start_page_size == buf.len() {
            (0usize, 0usize)
        } else {
            (
                (buf.len() - start_page_size) / PAGE_SIZE_BYTES as usize,
                (buf.len() - start_page_size) % PAGE_SIZE_BYTES as usize,
            )
        };

        // write to first page
        self.pages[start_page_idx][start_page_inner_idx..(start_page_inner_idx + start_page_size)]
            .copy_from_slice(&buf[0..start_page_size]);

        // write to pages in-between
        for i in 0..pages_in_between {
            self.pages[start_page_idx + i + 1].copy_from_slice(
                &buf[(start_page_size + i * PAGE_SIZE_BYTES as usize)
                    ..(start_page_size + (i + 1) * PAGE_SIZE_BYTES as usize)],
            );
        }

        // write to last page
        if last_page_size == 0 {
            return;
        }

        self.pages[start_page_idx + pages_in_between + 1][0..last_page_size].copy_from_slice(
            &buf[(start_page_size + pages_in_between * PAGE_SIZE_BYTES as usize)
                ..(start_page_size + pages_in_between * PAGE_SIZE_BYTES as usize + last_page_size)],
        );
    }
}

#[cfg(target_family = "wasm")]
pub mod stable {
    use crate::utils::mem_context::{MemContext, OutOfMemory, StableMemContext};

    #[inline]
    pub fn size_pages() -> u64 {
        MemContext::size_pages(&StableMemContext)
    }

    #[inline]
    pub fn grow(new_pages: u64) -> Result<u64, OutOfMemory> {
        MemContext::grow(&mut StableMemContext, new_pages)
    }

    #[inline]
    pub fn read(offset: u64, buf: &mut [u8]) {
        MemContext::read(&StableMemContext, offset, buf)
    }

    #[inline]
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

    #[inline]
    pub fn clear() {
        CONTEXT.with(|it| it.borrow_mut().pages.clear())
    }

    #[inline]
    pub fn size_pages() -> u64 {
        CONTEXT.with(|it| it.borrow().size_pages())
    }

    #[inline]
    pub fn grow(new_pages: u64) -> Result<u64, OutOfMemory> {
        CONTEXT.with(|it| it.borrow_mut().grow(new_pages))
    }

    #[inline]
    pub fn read(offset: u64, buf: &mut [u8]) {
        CONTEXT.with(|it| it.borrow().read(offset, buf))
    }

    #[inline]
    pub fn write(offset: u64, buf: &[u8]) {
        CONTEXT.with(|it| it.borrow_mut().write(offset, buf))
    }
}

#[cfg(test)]
mod tests {
    use crate::{stable, PAGE_SIZE_BYTES};
    use rand::seq::SliceRandom;
    use rand::{thread_rng, Rng};

    #[test]
    fn random_works_fine() {
        for _ in 0..100 {
            stable::clear();
            stable::grow(1000).unwrap();

            let mut rng = thread_rng();
            let iterations = 500usize;
            let size_range = (0..(u16::MAX as usize * 2));

            let mut sizes = Vec::new();
            let mut cur_ptr = 0;
            for i in 0..iterations {
                let size = rng.gen_range(size_range.clone());
                let buf = vec![(i % 256) as u8; size];

                stable::write(cur_ptr, &buf);

                sizes.push(size);
                cur_ptr += size as u64;

                let mut c_ptr = 0u64;
                for j in 0..i {
                    let size = sizes[j];
                    let mut buf = vec![0u8; size];

                    stable::read(c_ptr, &mut buf);

                    assert_eq!(buf, vec![(j % 256) as u8; size]);

                    c_ptr += size as u64;
                }
            }
        }
    }

    #[test]
    fn big_reads_writes_work_fine() {
        stable::clear();
        stable::grow(10).unwrap();

        let buf = [10u8; PAGE_SIZE_BYTES as usize * 10];
        stable::write(0, &buf);

        let mut buf1 = [0u8; PAGE_SIZE_BYTES as usize * 10 - 50];
        stable::read(25, &mut buf1);

        assert_eq!(buf[25..PAGE_SIZE_BYTES as usize * 10 - 25], buf1);
    }
}

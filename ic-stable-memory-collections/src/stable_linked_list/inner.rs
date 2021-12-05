use crate::types::StableLinkedListError;
use ic_stable_memory_allocator::mem_block::{MemBlock, MemBlockSide};
use ic_stable_memory_allocator::mem_context::MemContext;
use ic_stable_memory_allocator::stable_memory_allocator::StableMemoryAllocator;
use ic_stable_memory_allocator::types::{SMAError, EMPTY_PTR};
use std::marker::PhantomData;
use std::mem::size_of;

pub const STABLE_LINKED_LIST_MARKER: [u8; 1] = [2];

// TODO: убрать все поля из структур - заменить на геттеры-сеттеры

pub struct StableLinkedListInner<T: MemContext + Clone> {
    ptr: u64,
    marker: PhantomData<T>,
}

impl<T: MemContext + Clone> StableLinkedListInner<T> {
    pub fn new(
        allocator: &mut StableMemoryAllocator<T>,
        context: &mut T,
    ) -> Result<Self, StableLinkedListError> {
        let mut mem_block = allocator
            .allocate(1 + size_of::<u64>() as u64 * 3, context)
            .map_err(StableLinkedListError::SMAError)?;

        mem_block
            .write_bytes(0, &STABLE_LINKED_LIST_MARKER, context)
            .unwrap();
        mem_block.write_u64(1, 0, context).unwrap();
        mem_block
            .write_u64(1 + size_of::<u64>() as u64, EMPTY_PTR, context)
            .unwrap();
        mem_block
            .write_u64(1 + size_of::<u64>() as u64 * 2, EMPTY_PTR, context)
            .unwrap();

        Ok(Self {
            ptr: mem_block.ptr,
            marker: PhantomData,
        })
    }

    pub fn read_at(ptr: u64, context: &T) -> Result<Self, StableLinkedListError> {
        let mem_block = MemBlock::read_at(ptr, MemBlockSide::Start, context).ok_or(
            StableLinkedListError::SMAError(SMAError::NoMemBlockAtAddress),
        )?;

        let mut marker_buf = [0u8; 1];
        mem_block
            .read_bytes(0, &mut marker_buf, context)
            .map_err(StableLinkedListError::SMAError)?;

        if marker_buf != STABLE_LINKED_LIST_MARKER {
            return Err(StableLinkedListError::MarkerMismatch);
        }

        Ok(Self {
            ptr,
            marker: PhantomData,
        })
    }

    fn set_first(&mut self, new_first_opt: Option<u64>, context: &mut T) {
        let mut mem_block = MemBlock::read_at(self.ptr, MemBlockSide::Start, context).unwrap();

        let new_first = if let Some(n) = new_first_opt {
            n
        } else {
            EMPTY_PTR
        };

        mem_block
            .write_u64(1 + size_of::<u64>() as u64, new_first, context)
            .unwrap();
    }

    pub fn get_first(&self, context: &T) -> Option<u64> {
        let mem_block = MemBlock::read_at(self.ptr, MemBlockSide::Start, context).unwrap();

        let first = mem_block
            .read_u64(1 + size_of::<u64>() as u64, context)
            .unwrap();

        if first == EMPTY_PTR {
            return None;
        }

        Some(first)
    }

    fn set_last(&mut self, new_last_opt: Option<u64>, context: &mut T) {
        let mut mem_block = MemBlock::read_at(self.ptr, MemBlockSide::Start, context).unwrap();

        let new_last = if let Some(n) = new_last_opt {
            n
        } else {
            EMPTY_PTR
        };

        mem_block
            .write_u64(1 + size_of::<u64>() as u64 * 2, new_last, context)
            .unwrap();
    }

    pub fn get_last(&self, context: &T) -> Option<u64> {
        let mem_block = MemBlock::read_at(self.ptr, MemBlockSide::Start, context).unwrap();

        let last = mem_block
            .read_u64(1 + size_of::<u64>() as u64 * 2, context)
            .unwrap();

        if last == EMPTY_PTR {
            return None;
        }

        Some(last)
    }

    fn set_len(&mut self, new_len: u64, context: &mut T) {
        let mut mem_block = MemBlock::read_at(self.ptr, MemBlockSide::Start, context).unwrap();

        mem_block.write_u64(1, new_len, context).unwrap();
    }

    pub fn get_len(&self, context: &T) -> u64 {
        let mem_block = MemBlock::read_at(self.ptr, MemBlockSide::Start, context).unwrap();

        mem_block.read_u64(1, context).unwrap()
    }

    pub fn push_first(
        &mut self,
        value: &[u8],
        allocator: &mut StableMemoryAllocator<T>,
        context: &mut T,
    ) -> Result<(), StableLinkedListError> {
        let mut item =
            StableLinkedListInnerItem::new(value.len(), EMPTY_PTR, EMPTY_PTR, allocator, context)?;

        item.set_data(0, value, context).unwrap();

        if self.get_first(context).is_none() && self.get_last(context).is_none() {
            self.set_first(Some(item.ptr), context);
            self.set_last(Some(item.ptr), context);
        } else {
            let mut prev_first =
                StableLinkedListInnerItem::read_at(self.get_first(context).unwrap(), context)
                    .unwrap();
            prev_first.set_prev(Some(item.ptr), context);

            item.set_next(Some(prev_first.ptr), context);

            self.set_first(Some(item.ptr), context);
        }

        self.set_len(self.get_len(context) + 1, context);

        Ok(())
    }

    pub fn push_last(
        &mut self,
        value: &[u8],
        allocator: &mut StableMemoryAllocator<T>,
        context: &mut T,
    ) -> Result<(), StableLinkedListError> {
        let mut item =
            StableLinkedListInnerItem::new(value.len(), EMPTY_PTR, EMPTY_PTR, allocator, context)?;

        item.set_data(0, value, context).unwrap();

        if self.get_first(context).is_none() && self.get_last(context).is_none() {
            self.set_first(Some(item.ptr), context);
        } else {
            let mut prev_last =
                StableLinkedListInnerItem::read_at(self.get_last(context).unwrap(), context)
                    .unwrap();

            prev_last.set_next(Some(item.ptr), context);

            item.set_prev(Some(prev_last.ptr), context);
        }

        self.set_last(Some(item.ptr), context);

        self.set_len(self.get_len(context) + 1, context);

        Ok(())
    }

    pub fn pop_first(
        &mut self,
        allocator: &mut StableMemoryAllocator<T>,
        context: &mut T,
    ) -> Option<Vec<u8>> {
        self.get_first(context)?;

        let prev_first =
            StableLinkedListInnerItem::read_at(self.get_first(context).unwrap(), context).unwrap();

        if self.get_first(context) == self.get_last(context) {
            self.set_first(None, context);
            self.set_last(None, context);
        } else {
            self.set_first(prev_first.get_next(context), context);
        }

        if let Some(prev_first_next) = prev_first.get_next(context) {
            let mut new_first =
                StableLinkedListInnerItem::read_at(prev_first_next, context).unwrap();

            new_first.set_prev(None, context);
        }

        self.set_len(self.get_len(context) - 1, context);

        let mut buf = vec![0u8; prev_first.get_size(context)];
        prev_first.get_data(0, &mut buf, context).unwrap();

        prev_first.delete(allocator, context);

        Some(buf)
    }

    pub fn pop_last(
        &mut self,
        allocator: &mut StableMemoryAllocator<T>,
        context: &mut T,
    ) -> Option<Vec<u8>> {
        self.get_last(context)?;

        let prev_last =
            StableLinkedListInnerItem::read_at(self.get_last(context).unwrap(), context).unwrap();

        if self.get_first(context) == self.get_last(context) {
            self.set_first(None, context);
            self.set_last(None, context);
        } else {
            self.set_last(prev_last.get_prev(context), context);
        }

        if let Some(prev_last_prev) = prev_last.get_prev(context) {
            let mut new_last = StableLinkedListInnerItem::read_at(prev_last_prev, context).unwrap();

            new_last.set_next(None, context);
        }

        self.set_len(self.get_len(context) - 1, context);

        let mut buf = vec![0u8; prev_last.get_size(context)];
        prev_last.get_data(0, &mut buf, context).unwrap();

        prev_last.delete(allocator, context);

        Some(buf)
    }
}

pub struct StableLinkedListInnerItem<T: MemContext + Clone> {
    ptr: u64,
    marker: PhantomData<T>,
}

impl<T: MemContext + Clone> StableLinkedListInnerItem<T> {
    pub(crate) fn new(
        size: usize,
        prev: u64,
        next: u64,
        allocator: &mut StableMemoryAllocator<T>,
        context: &mut T,
    ) -> Result<Self, StableLinkedListError> {
        let mut mem_block = allocator
            .allocate((size_of::<u64>() * 2 + size) as u64, context)
            .map_err(StableLinkedListError::SMAError)?;

        mem_block.write_u64(0, prev, context).unwrap();
        mem_block
            .write_u64(size_of::<u64>() as u64, next, context)
            .unwrap();

        Ok(Self {
            ptr: mem_block.ptr,
            marker: PhantomData,
        })
    }

    pub(crate) fn read_at(ptr: u64, context: &T) -> Result<Self, StableLinkedListError> {
        MemBlock::read_at(ptr, MemBlockSide::Start, context).ok_or(
            StableLinkedListError::SMAError(SMAError::NoMemBlockAtAddress),
        )?;

        Ok(Self {
            ptr,
            marker: PhantomData,
        })
    }

    pub(crate) fn delete(self, allocator: &mut StableMemoryAllocator<T>, context: &mut T) {
        allocator.deallocate(self.ptr, context);
    }

    pub fn get_size(&self, context: &T) -> usize {
        let mem_block = MemBlock::read_at(self.ptr, MemBlockSide::Start, context).unwrap();

        mem_block.size as usize - size_of::<u64>() * 2
    }

    pub fn get_data(
        &self,
        offset: usize,
        buf: &mut [u8],
        context: &T,
    ) -> Result<(), StableLinkedListError> {
        let mem_block = MemBlock::read_at(self.ptr, MemBlockSide::Start, context).unwrap();

        if size_of::<u64>() * 2 + offset + buf.len() > mem_block.size as usize {
            return Err(StableLinkedListError::SMAError(SMAError::OutOfBounds));
        }

        mem_block
            .read_bytes((size_of::<u64>() * 2 + offset) as u64, buf, context)
            .map_err(StableLinkedListError::SMAError)
    }

    pub fn set_data(
        &mut self,
        offset: usize,
        buf: &[u8],
        context: &mut T,
    ) -> Result<(), StableLinkedListError> {
        let mut mem_block = MemBlock::read_at(self.ptr, MemBlockSide::Start, context).unwrap();

        if size_of::<u64>() * 2 + offset + buf.len() > mem_block.size as usize {
            return Err(StableLinkedListError::SMAError(SMAError::OutOfBounds));
        }

        mem_block
            .write_bytes((size_of::<u64>() * 2 + offset) as u64, buf, context)
            .map_err(StableLinkedListError::SMAError)
    }

    pub fn get_prev_item(&self, context: &T) -> Option<Self> {
        let prev = self.get_prev(context)?;

        Self::read_at(prev, context).ok()
    }

    pub fn get_next_item(&self, context: &T) -> Option<Self> {
        let next = self.get_next(context)?;

        Self::read_at(next, context).ok()
    }

    pub(crate) fn get_prev(&self, context: &T) -> Option<u64> {
        let mem_block = MemBlock::read_at(self.ptr, MemBlockSide::Start, context).unwrap();

        let prev = mem_block.read_u64(0, context).unwrap();

        if prev == EMPTY_PTR {
            return None;
        }

        Some(prev)
    }

    pub(crate) fn set_prev(&mut self, new_prev_opt: Option<u64>, context: &mut T) {
        let mut mem_block = MemBlock::read_at(self.ptr, MemBlockSide::Start, context).unwrap();

        let new_prev = if let Some(n) = new_prev_opt {
            n
        } else {
            EMPTY_PTR
        };

        mem_block.write_u64(0, new_prev, context).unwrap();
    }

    pub(crate) fn get_next(&self, context: &T) -> Option<u64> {
        let mem_block = MemBlock::read_at(self.ptr, MemBlockSide::Start, context).unwrap();

        let next = mem_block
            .read_u64(size_of::<u64>() as u64, context)
            .unwrap();

        if next == EMPTY_PTR {
            return None;
        }

        Some(next)
    }

    pub(crate) fn set_next(&mut self, new_next_opt: Option<u64>, context: &mut T) {
        let mut mem_block = MemBlock::read_at(self.ptr, MemBlockSide::Start, context).unwrap();

        let new_next = if let Some(n) = new_next_opt {
            n
        } else {
            EMPTY_PTR
        };

        mem_block
            .write_u64(size_of::<u64>() as u64, new_next, context)
            .unwrap();
    }
}

#[cfg(test)]
mod tests {
    use crate::stable_linked_list::inner::{StableLinkedListInner, StableLinkedListInnerItem};
    use ic_stable_memory_allocator::mem_context::TestMemContext;
    use ic_stable_memory_allocator::stable_memory_allocator::StableMemoryAllocator;

    #[test]
    fn linked_list_works_fine() {
        let mut context = TestMemContext::default();
        let mut allocator = StableMemoryAllocator::init(0, &mut context).ok().unwrap();
        let mut linked_list = StableLinkedListInner::new(&mut allocator, &mut context)
            .ok()
            .unwrap();

        linked_list
            .push_first(&[1, 3, 3, 7], &mut allocator, &mut context)
            .ok()
            .unwrap();

        let first_ptr = linked_list.get_first(&context).unwrap();
        let first = StableLinkedListInnerItem::read_at(first_ptr, &context).unwrap();

        assert_eq!(
            linked_list.get_first(&context),
            linked_list.get_last(&context),
            "Linked list should have first=last"
        );
        assert_eq!(
            linked_list.get_first(&context).unwrap(),
            first.ptr,
            "First should match linked-list first and last"
        );

        linked_list
            .push_last(&[1, 3, 3, 8], &mut allocator, &mut context)
            .ok()
            .unwrap();

        let last_ptr = linked_list.get_last(&context).unwrap();
        let last = StableLinkedListInnerItem::read_at(last_ptr, &context).unwrap();

        assert_eq!(
            linked_list.get_first(&context).unwrap(),
            first.ptr,
            "First doesn't match"
        );
        assert_eq!(
            linked_list.get_last(&context).unwrap(),
            last.ptr,
            "Last doesn't match"
        );
        assert_eq!(
            first.get_next(&context).unwrap(),
            last.ptr,
            "First-last doesn't match"
        );
        assert_eq!(
            last.get_prev(&context).unwrap(),
            first.ptr,
            "Last-first doesn't match"
        );

        let first_content = linked_list.pop_first(&mut allocator, &mut context).unwrap();
        assert_eq!(first_content, vec![1u8, 3, 3, 7], "First content mismatch");

        let first_ptr = linked_list.get_first(&context).unwrap();
        let first = StableLinkedListInnerItem::read_at(first_ptr, &context).unwrap();

        assert_eq!(first_ptr, last_ptr, "First should match last 1");

        let last_ptr = linked_list.get_last(&context).unwrap();
        let last = StableLinkedListInnerItem::read_at(last_ptr, &context).unwrap();

        assert_eq!(first_ptr, last_ptr, "First should match last 2");

        let last_content = linked_list.pop_first(&mut allocator, &mut context).unwrap();
        assert_eq!(last_content, vec![1u8, 3, 3, 8], "Last content mismatch");

        let first_ptr = linked_list.get_first(&context);
        let last_ptr = linked_list.get_last(&context);

        assert!(first_ptr.is_none(), "There should be no first");
        assert!(last_ptr.is_none(), "There should be no last");
    }
}

use crate::collections::log::iter::SLogIter;
use crate::encoding::AsFixedSizeBytes;
use crate::mem::allocator::EMPTY_PTR;
use crate::mem::StablePtr;
use crate::primitive::s_ref::SRef;
use crate::primitive::s_ref_mut::SRefMut;
use crate::primitive::StableType;
use crate::{allocate, deallocate, OutOfMemory, SSlice};
use std::fmt::Debug;
use std::marker::PhantomData;

#[doc(hidden)]
pub mod iter;

pub(crate) const DEFAULT_CAPACITY: u64 = 2;

/// Non-reallocating growing vector optimized for storing logs or history entries
///
/// Very similar to [SVec](crate::collections::SVec), but internally does not perform reallocations
/// nor moves of data. Instead, when the capacity is reached, a new block of stable memory (which
/// here is called a `Sector`) is allocated and attached to the end of the current block. So, in
/// some sense, this is a linked list, where each node can hold multiple elements.
///
/// `T` has to implement both [StableType] and [AsFixedSizeBytes]. [SLog] itself also implements these
/// trait, which means that you can store it inside an another stable structure.
///
/// This data structure is "infinite" - it can handle up to [u64::MAX] elements.
///
/// [SLog] grows exponentially - each new `Sector` is twice as big as the previous one. But if the
/// canister is short on stable memory, the newly created `Sector` may be shrunk, to be able to continue
/// to grow.
///
/// It is well optimized when you access elements near the end (the most recently added). The further
/// the element you access from the end, the worser the performance.
pub struct SLog<T: StableType + AsFixedSizeBytes> {
    len: u64,
    first_sector_ptr: StablePtr,
    cur_sector_ptr: StablePtr,
    cur_sector_last_item_offset: u64,
    cur_sector_capacity: u64,
    cur_sector_len: u64,
    stable_drop_flag: bool,
    _marker: PhantomData<T>,
}

impl<T: StableType + AsFixedSizeBytes> SLog<T> {
    /// Creates a new [SLog]
    ///
    /// Does not allocate any heap or stable memory.
    #[inline]
    pub fn new() -> Self {
        Self {
            len: 0,
            first_sector_ptr: EMPTY_PTR,
            cur_sector_ptr: EMPTY_PTR,
            cur_sector_last_item_offset: 0,
            cur_sector_capacity: DEFAULT_CAPACITY,
            cur_sector_len: 0,
            stable_drop_flag: true,
            _marker: PhantomData::default(),
        }
    }

    /// Inserts a new element at the end of the [SLog]
    ///
    /// May allocate a new `Sector`. If the canister is out of stable memory, will return [Err] with
    /// the element that was about to get inserted.
    ///
    /// # Example
    /// ```rust
    /// # use ic_stable_memory::collections::SLog;
    /// # use ic_stable_memory::stable_memory_init;
    /// # unsafe { ic_stable_memory::mem::clear(); }
    /// # stable_memory_init();
    /// let mut log = SLog::new();
    ///
    /// log.push(10u64).expect("Out of memory");
    /// ```
    pub fn push(&mut self, it: T) -> Result<(), T> {
        if let Ok(mut sector) = self.get_or_create_current_sector() {
            if self.move_to_next_sector_if_needed(&mut sector).is_ok() {
                sector.write_and_own_element(self.cur_sector_last_item_offset, it);
                self.cur_sector_last_item_offset += T::SIZE as u64;
                self.cur_sector_len += 1;
                self.len += 1;

                Ok(())
            } else {
                Err(it)
            }
        } else {
            Err(it)
        }
    }

    /// Removes an element from the end of the [SLog]
    ///
    /// If the [SLog] is empty, returns [None]. If it was the last element of the last `Sector` and
    /// there are more `Sectors` before it, the last `Sector` gets deallocated, freeing the memory.
    pub fn pop(&mut self) -> Option<T> {
        if self.len == 0 {
            return None;
        }

        let sector = self.get_current_sector()?;

        self.cur_sector_last_item_offset -= T::SIZE as u64;
        self.cur_sector_len -= 1;
        self.len -= 1;

        let it = sector.read_and_disown_element(self.cur_sector_last_item_offset);

        self.move_to_prev_sector_if_needed(sector);

        Some(it)
    }

    /// Removes all elements from this [SLog]
    ///
    /// Deallocates all `Sectors`, but the first one, freeing the memory.
    #[inline]
    pub fn clear(&mut self) {
        while self.pop().is_some() {}
    }

    /// Returns an immutable reference [SRef] to the last element of this [SLog]
    ///
    /// If the [SLog] is empty, returns [None].
    ///
    /// # Example
    /// ```rust
    /// # use ic_stable_memory::collections::SLog;
    /// # use ic_stable_memory::stable_memory_init;
    /// # unsafe { ic_stable_memory::mem::clear(); }
    /// # stable_memory_init();
    /// let mut log = SLog::new();
    ///
    /// log.push(10u64).expect("Out of memory");
    ///
    /// assert_eq!(*log.last().unwrap(), 10);
    /// ```
    pub fn last(&self) -> Option<SRef<T>> {
        if self.len == 0 {
            return None;
        }

        let sector = self.get_current_sector()?;
        let ptr = sector.get_element_ptr(self.cur_sector_last_item_offset - T::SIZE as u64);

        unsafe { Some(SRef::new(ptr)) }
    }

    /// Efficiently returns an immutable reference [SRef] to the first element of this [SLog]
    ///
    /// If the [SLog] is empty, returns [None].
    ///
    /// # Example
    /// ```rust
    /// # use ic_stable_memory::collections::SLog;
    /// # use ic_stable_memory::stable_memory_init;
    /// # unsafe { ic_stable_memory::mem::clear(); }
    /// # stable_memory_init();
    /// let mut log = SLog::new();
    ///
    /// log.push(10u64).expect("Out of memory");
    ///
    /// assert_eq!(*log.first().unwrap(), 10);
    /// ```
    pub fn first(&self) -> Option<SRef<T>> {
        if self.len == 0 {
            return None;
        }

        let sector = self.get_first_sector()?;
        let ptr = sector.get_element_ptr(0);

        unsafe { Some(SRef::new(ptr)) }
    }

    /// Returns an immutable reference [SRef] to an element at the requested index
    ///
    /// See also [SLog::get_mut].
    ///
    /// The closer the index to `0`, the worser the performance of this call.
    ///
    /// If the [SLog] is empty, returns [None]
    #[inline]
    pub fn get(&self, idx: u64) -> Option<SRef<T>> {
        let (sector, dif) = self.find_sector_for_idx(idx)?;
        let ptr = sector.get_element_ptr((idx - dif) * T::SIZE as u64);

        unsafe { Some(SRef::new(ptr)) }
    }

    /// Returns a mutable reference [SRefMut] to an element at the requested index
    ///
    /// See also [SLog::get].
    ///
    /// The closer the index to `0`, the worser the performance of this call.
    ///
    /// If the [SLog] is empty, returns [None]
    #[inline]
    pub fn get_mut(&mut self, idx: u64) -> Option<SRefMut<T>> {
        let (sector, dif) = self.find_sector_for_idx(idx)?;
        let ptr = sector.get_element_ptr((idx - dif) * T::SIZE as u64);

        unsafe { Some(SRefMut::new(ptr)) }
    }

    /// Returns the length of this [SLog]
    #[inline]
    pub fn len(&self) -> u64 {
        self.len
    }

    /// Returns true if the length of this [SLog] is `0`
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Returns a back-to-front iterator over this [SLog]
    ///
    /// This iterator contains elements from last to first.
    ///
    /// # Example
    /// ```rust
    /// # use ic_stable_memory::collections::SLog;
    /// # use ic_stable_memory::stable_memory_init;
    /// # unsafe { ic_stable_memory::mem::clear(); }
    /// # stable_memory_init();
    /// let mut log = SLog::new();
    ///
    /// for i in 0..100 {
    ///     log.push(i).expect("Out of memory");
    /// }
    ///
    /// let mut i = 99;
    /// for elem in log.rev_iter() {
    ///     assert_eq!(*elem, i);
    ///     i -= 1;
    /// }
    /// ```
    #[inline]
    pub fn rev_iter(&self) -> SLogIter<'_, T> {
        SLogIter::new(self)
    }

    fn find_sector_for_idx(&self, idx: u64) -> Option<(Sector<T>, u64)> {
        if idx >= self.len || self.len == 0 {
            return None;
        }

        let mut sector = Sector::<T>::from_ptr(self.cur_sector_ptr);
        let mut sector_len = self.cur_sector_len;

        let mut len = self.len;

        loop {
            len -= sector_len;
            if len <= idx {
                break;
            }

            sector = Sector::<T>::from_ptr(sector.read_prev_ptr());
            sector_len = sector.read_capacity();
        }

        Some((sector, len))
    }

    fn get_or_create_current_sector(&mut self) -> Result<Sector<T>, OutOfMemory> {
        if self.cur_sector_ptr == EMPTY_PTR {
            self.cur_sector_capacity *= 2;

            let it = Sector::<T>::new(self.cur_sector_capacity, EMPTY_PTR)?;

            self.first_sector_ptr = it.as_ptr();
            self.cur_sector_ptr = it.as_ptr();

            Ok(it)
        } else {
            Ok(Sector::<T>::from_ptr(self.cur_sector_ptr))
        }
    }

    #[inline]
    fn get_current_sector(&self) -> Option<Sector<T>> {
        if self.cur_sector_ptr == EMPTY_PTR {
            None
        } else {
            Some(Sector::<T>::from_ptr(self.cur_sector_ptr))
        }
    }

    #[inline]
    fn get_first_sector(&self) -> Option<Sector<T>> {
        if self.first_sector_ptr == EMPTY_PTR {
            None
        } else {
            Some(Sector::<T>::from_ptr(self.first_sector_ptr))
        }
    }

    fn move_to_prev_sector_if_needed(&mut self, sector: Sector<T>) {
        if self.cur_sector_len > 0 {
            return;
        }

        let prev_sector_ptr = sector.read_prev_ptr();
        if prev_sector_ptr == EMPTY_PTR {
            return;
        }

        let cur_sector = Sector::<T>::from_ptr(self.cur_sector_ptr);
        cur_sector.destroy();

        let mut prev_sector = Sector::<T>::from_ptr(prev_sector_ptr);
        prev_sector.write_next_ptr(EMPTY_PTR);

        self.cur_sector_capacity = prev_sector.read_capacity();
        self.cur_sector_len = self.cur_sector_capacity;
        self.cur_sector_ptr = prev_sector_ptr;
        self.cur_sector_last_item_offset = self.cur_sector_capacity * T::SIZE as u64;
    }

    fn move_to_next_sector_if_needed(&mut self, sector: &mut Sector<T>) -> Result<(), OutOfMemory> {
        if self.cur_sector_len < self.cur_sector_capacity {
            return Ok(());
        }

        let mut next_sector_capacity = self.cur_sector_capacity.checked_mul(2).unwrap();
        let mut new_sector = loop {
            if next_sector_capacity <= DEFAULT_CAPACITY {
                return Err(OutOfMemory);
            }

            match Sector::<T>::new(next_sector_capacity, sector.as_ptr()) {
                Ok(s) => break s,
                Err(_) => {
                    next_sector_capacity /= 2;
                    continue;
                }
            };
        };

        sector.write_next_ptr(new_sector.as_ptr());
        new_sector.write_prev_ptr(sector.as_ptr());

        self.cur_sector_capacity = next_sector_capacity;
        self.cur_sector_ptr = new_sector.as_ptr();
        self.cur_sector_len = 0;
        self.cur_sector_last_item_offset = 0;

        *sector = new_sector;

        Ok(())
    }
}

impl<T: StableType + AsFixedSizeBytes> Default for SLog<T> {
    fn default() -> Self {
        Self::new()
    }
}

const PREV_OFFSET: u64 = 0;
const NEXT_OFFSET: u64 = PREV_OFFSET + u64::SIZE as u64;
const CAPACITY_OFFSET: u64 = NEXT_OFFSET + u64::SIZE as u64;
const ELEMENTS_OFFSET: u64 = CAPACITY_OFFSET + u64::SIZE as u64;

struct Sector<T>(u64, PhantomData<T>);

impl<T: StableType + AsFixedSizeBytes> Sector<T> {
    fn new(cap: u64, prev: StablePtr) -> Result<Self, OutOfMemory> {
        let slice = unsafe { allocate(u64::SIZE as u64 * 3 + cap * T::SIZE as u64)? };

        let mut it = Self(slice.as_ptr(), PhantomData::default());
        it.write_prev_ptr(prev);
        it.write_next_ptr(EMPTY_PTR);
        it.write_capacity(cap);

        Ok(it)
    }

    fn destroy(self) {
        let slice = unsafe { SSlice::from_ptr(self.0).unwrap() };
        deallocate(slice);
    }

    #[inline]
    fn as_ptr(&self) -> StablePtr {
        self.0
    }

    #[inline]
    fn from_ptr(ptr: u64) -> Self {
        Self(ptr, PhantomData::default())
    }

    #[inline]
    fn read_prev_ptr(&self) -> StablePtr {
        unsafe { crate::mem::read_fixed_for_reference(SSlice::_offset(self.0, PREV_OFFSET)) }
    }

    #[inline]
    fn write_prev_ptr(&mut self, mut ptr: StablePtr) {
        unsafe { crate::mem::write_fixed(SSlice::_offset(self.0, PREV_OFFSET), &mut ptr) }
    }

    #[inline]
    fn read_next_ptr(&self) -> StablePtr {
        unsafe { crate::mem::read_fixed_for_reference(SSlice::_offset(self.0, NEXT_OFFSET)) }
    }

    #[inline]
    fn write_next_ptr(&mut self, mut ptr: StablePtr) {
        unsafe { crate::mem::write_fixed(SSlice::_offset(self.0, NEXT_OFFSET), &mut ptr) }
    }

    #[inline]
    fn read_capacity(&self) -> u64 {
        unsafe { crate::mem::read_fixed_for_reference(SSlice::_offset(self.0, CAPACITY_OFFSET)) }
    }

    #[inline]
    fn write_capacity(&mut self, mut cap: u64) {
        unsafe { crate::mem::write_fixed(SSlice::_offset(self.0, CAPACITY_OFFSET), &mut cap) }
    }

    #[inline]
    fn get_element_ptr(&self, offset: u64) -> u64 {
        SSlice::_offset(self.0, ELEMENTS_OFFSET + offset)
    }

    #[inline]
    fn read_and_disown_element(&self, offset: u64) -> T {
        unsafe { crate::mem::read_fixed_for_move(self.get_element_ptr(offset)) }
    }

    #[inline]
    fn get_element(&self, offset: u64) -> SRef<T> {
        unsafe { SRef::new(self.get_element_ptr(offset)) }
    }

    #[inline]
    fn get_element_mut(&mut self, offset: u64) -> SRefMut<T> {
        unsafe { SRefMut::new(self.get_element_ptr(offset)) }
    }

    #[inline]
    fn write_and_own_element(&self, offset: u64, mut element: T) {
        unsafe { crate::mem::write_fixed(self.get_element_ptr(offset), &mut element) };
    }
}

impl<T: StableType + AsFixedSizeBytes + Debug> SLog<T> {
    /// Prints sectored representation of this [SLog]
    ///
    /// Useful for tests
    pub fn debug_print(&self) {
        let mut sector = if let Some(s) = self.get_first_sector() {
            s
        } else {
            println!("SLog []");
            return;
        };

        let mut current_sector_len = DEFAULT_CAPACITY * 2;

        print!(
            "SLog({}, {}, {}, {}, {}, {})",
            self.len,
            self.first_sector_ptr,
            self.cur_sector_ptr,
            self.cur_sector_len,
            self.cur_sector_capacity,
            self.cur_sector_last_item_offset
        );

        print!(" [");

        loop {
            print!("[");
            let len = if sector.as_ptr() == self.cur_sector_ptr {
                self.cur_sector_len
            } else {
                current_sector_len
            };

            let mut offset = 0;
            for i in 0..len {
                let elem = sector.get_element(offset);
                offset += T::SIZE as u64;

                print!("{:?}", *elem);
                if i < len - 1 {
                    print!(", ");
                }
            }
            print!("]");

            if sector.as_ptr() == self.cur_sector_ptr {
                break;
            }

            print!(", ");

            let next_sector_ptr = sector.read_next_ptr();
            assert_ne!(next_sector_ptr, EMPTY_PTR);

            sector = Sector::<T>::from_ptr(next_sector_ptr);
            current_sector_len *= 2;
        }

        println!("]");
    }
}

impl<T: StableType + AsFixedSizeBytes> AsFixedSizeBytes for SLog<T> {
    const SIZE: usize = u64::SIZE * 6 + usize::SIZE;
    type Buf = [u8; u64::SIZE * 6 + usize::SIZE];

    fn as_fixed_size_bytes(&self, buf: &mut [u8]) {
        self.len.as_fixed_size_bytes(&mut buf[0..u64::SIZE]);
        self.first_sector_ptr
            .as_fixed_size_bytes(&mut buf[u64::SIZE..(u64::SIZE * 2)]);
        self.cur_sector_ptr
            .as_fixed_size_bytes(&mut buf[(u64::SIZE * 2)..(u64::SIZE * 3)]);
        self.cur_sector_last_item_offset
            .as_fixed_size_bytes(&mut buf[(u64::SIZE * 3)..(u64::SIZE * 4)]);
        self.cur_sector_capacity
            .as_fixed_size_bytes(&mut buf[(u64::SIZE * 4)..(u64::SIZE * 5)]);
        self.cur_sector_len
            .as_fixed_size_bytes(&mut buf[(u64::SIZE * 5)..(u64::SIZE * 6)]);
    }

    fn from_fixed_size_bytes(buf: &[u8]) -> Self {
        let len = u64::from_fixed_size_bytes(&buf[0..u64::SIZE]);
        let first_sector_ptr = u64::from_fixed_size_bytes(&buf[u64::SIZE..(u64::SIZE * 2)]);
        let cur_sector_ptr = u64::from_fixed_size_bytes(&buf[(u64::SIZE * 2)..(u64::SIZE * 3)]);
        let cur_sector_last_item_offset =
            u64::from_fixed_size_bytes(&buf[(u64::SIZE * 3)..(u64::SIZE * 4)]);
        let cur_sector_capacity =
            u64::from_fixed_size_bytes(&buf[(u64::SIZE * 4)..(u64::SIZE * 5)]);
        let cur_sector_len = u64::from_fixed_size_bytes(&buf[(u64::SIZE * 5)..(u64::SIZE * 6)]);

        Self {
            len,
            first_sector_ptr,
            cur_sector_ptr,
            cur_sector_len,
            cur_sector_capacity,
            cur_sector_last_item_offset,
            stable_drop_flag: false,
            _marker: PhantomData::default(),
        }
    }
}

impl<T: StableType + AsFixedSizeBytes> StableType for SLog<T> {
    #[inline]
    unsafe fn stable_drop_flag_off(&mut self) {
        self.stable_drop_flag = false;
    }

    #[inline]
    unsafe fn stable_drop_flag_on(&mut self) {
        self.stable_drop_flag = true;
    }

    #[inline]
    fn should_stable_drop(&self) -> bool {
        self.stable_drop_flag
    }

    #[inline]
    unsafe fn stable_drop(&mut self) {
        self.clear();

        if self.cur_sector_ptr != EMPTY_PTR {
            let sector = Sector::<T>::from_ptr(self.cur_sector_ptr);
            sector.destroy();
        }
    }
}

impl<T: StableType + AsFixedSizeBytes> Drop for SLog<T> {
    fn drop(&mut self) {
        if self.should_stable_drop() {
            unsafe {
                self.stable_drop();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::collections::log::SLog;
    use crate::utils::test::generate_random_string;
    use crate::{
        _debug_validate_allocator, get_allocated_size, init_allocator, retrieve_custom_data,
        stable, stable_memory_init, stable_memory_post_upgrade, stable_memory_pre_upgrade,
        store_custom_data, SBox,
    };
    use rand::rngs::ThreadRng;
    use rand::{thread_rng, Rng};

    #[test]
    fn works_fine() {
        stable::clear();
        stable_memory_init();

        {
            let mut log = SLog::new();

            assert!(log.is_empty());

            println!();
            println!("PUSHES");

            for i in 0..100 {
                log.debug_print();

                log.push(i);

                for j in 0..i {
                    assert_eq!(*log.get(j).unwrap(), j);
                }
            }

            log.debug_print();

            assert_eq!(log.len(), 100);
            for i in 0..100 {
                assert_eq!(*log.get(i).unwrap(), i);
            }

            println!();
            println!("POPS");

            for i in (20..100).rev() {
                assert_eq!(log.pop().unwrap(), i);
                log.debug_print();
            }

            println!();
            println!("PUSHES again");

            assert_eq!(log.len(), 20);
            for i in 20..100 {
                log.push(i);
                log.debug_print();
            }

            for i in 0..100 {
                assert_eq!(*log.get(i).unwrap(), i);
            }

            println!();
            println!("POPS again");

            for i in (0..100).rev() {
                assert_eq!(log.pop().unwrap(), i);
                log.debug_print();
            }

            assert!(log.pop().is_none());
            assert!(log.is_empty());
        }

        _debug_validate_allocator();
        assert_eq!(get_allocated_size(), 0);
    }

    #[test]
    fn iter_works_fine() {
        stable::clear();
        stable_memory_init();

        {
            let mut log = SLog::new();

            for i in 0..100 {
                log.push(i);
            }

            let mut j = 99;

            log.debug_print();

            for mut i in log.rev_iter() {
                assert_eq!(*i, j);
                j -= 1;
            }

            log.debug_print();
        }

        _debug_validate_allocator();
        assert_eq!(get_allocated_size(), 0);
    }

    enum Action {
        Push,
        Pop,
        Clear,
        CanisterUpgrade,
    }

    struct Fuzzer {
        state: Option<SLog<SBox<String>>>,
        example: Vec<String>,
        rng: ThreadRng,
        log: Vec<Action>,
    }

    impl Fuzzer {
        fn new() -> Self {
            Self {
                state: Some(SLog::default()),
                example: Vec::default(),
                rng: thread_rng(),
                log: Vec::default(),
            }
        }

        fn it(&mut self) -> &mut SLog<SBox<String>> {
            self.state.as_mut().unwrap()
        }

        fn next(&mut self) {
            let action = self.rng.gen_range(0..101);

            match action {
                // PUSH ~60%
                0..=60 => {
                    let str = generate_random_string(&mut self.rng);

                    if let Ok(data) = SBox::new(str.clone()) {
                        self.it().push(data);
                        self.example.push(str);

                        self.log.push(Action::Push);
                    }
                }
                // POP ~30%
                61..=90 => {
                    self.it().pop();
                    self.example.pop();

                    self.log.push(Action::Pop);
                }
                // CLEAR
                91..=92 => {
                    self.it().clear();
                    self.example.clear();

                    self.log.push(Action::Clear);
                }
                // CANISTER UPGRADE ~10%
                _ => match SBox::new(self.state.take().unwrap()) {
                    Ok(data) => {
                        store_custom_data(1, data);

                        if stable_memory_pre_upgrade().is_ok() {
                            stable_memory_post_upgrade();
                        }

                        self.state =
                            retrieve_custom_data::<SLog<SBox<String>>>(1).map(|it| it.into_inner());

                        self.log.push(Action::CanisterUpgrade);
                    }
                    Err(log) => {
                        self.state = Some(log);
                    }
                },
            }

            _debug_validate_allocator();
            assert_eq!(self.it().len(), self.example.len() as u64);

            for i in 0..self.it().len() {
                assert_eq!(
                    self.it().get(i).unwrap().clone(),
                    self.example.get(i as usize).unwrap().clone()
                );
            }
        }
    }

    #[test]
    fn fuzzer_works_fine() {
        stable::clear();
        init_allocator(0);

        {
            let mut fuzzer = Fuzzer::new();

            for _ in 0..10_000 {
                fuzzer.next();
            }
        }

        assert_eq!(get_allocated_size(), 0);
    }

    #[test]
    fn fuzzer_works_fine_limited_memory() {
        stable::clear();
        init_allocator(10);

        {
            let mut fuzzer = Fuzzer::new();

            for _ in 0..10_000 {
                fuzzer.next();
            }
        }

        assert_eq!(get_allocated_size(), 0);
    }
}

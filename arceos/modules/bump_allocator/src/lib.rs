#![no_std]

use allocator::{AllocError, AllocResult, BaseAllocator, ByteAllocator, PageAllocator};
use core::alloc::Layout;
use core::ptr::NonNull;

#[inline]
const fn align_down(pos: usize, align: usize) -> usize {
    pos & !(align - 1)
}

#[inline]
const fn align_up(pos: usize, align: usize) -> usize {
    (pos + align - 1) & !(align - 1)
}

/// Early memory allocator
/// Use it before formal bytes-allocator and pages-allocator can work!
/// This is a double-end memory range:
/// - Alloc bytes forward
/// - Alloc pages backward
///
/// [ bytes-used | avail-area | pages-used ]
/// |            | -->    <-- |            |
/// start       b_pos        p_pos       end
///
/// For bytes area, 'count' records number of allocations.
/// When it goes down to ZERO, free bytes-used area.
/// For pages area, it will never be freed!
///
pub struct EarlyAllocator<const PAGE_SIZE: usize> {
    /// Inclusive lower bound of the whole pool (aligned).
    start: usize,
    /// Exclusive upper bound of the whole pool (page-aligned).
    end: usize,
    /// Next byte bump position (in `[start, p_pos)`).
    b_pos: usize,
    /// Low boundary of the page region `[p_pos, end)` (grows downward on alloc).
    p_pos: usize,
    /// Number of outstanding `ByteAllocator::alloc` (not yet fully balanced by deallocs).
    byte_alloc_count: usize,
}

impl<const PAGE_SIZE: usize> EarlyAllocator<PAGE_SIZE> {
    pub const fn new() -> Self {
        Self {
            start: 0,
            end: 0,
            b_pos: 0,
            p_pos: 0,
            byte_alloc_count: 0,
        }
    }
}

impl<const PAGE_SIZE: usize> BaseAllocator for EarlyAllocator<PAGE_SIZE> {
    fn init(&mut self, start: usize, size: usize) {
        assert!(PAGE_SIZE.is_power_of_two());

        let raw_end = start.checked_add(size).expect("init: size overflow");
        let start = align_up(start, core::mem::size_of::<usize>());
        let end = align_down(raw_end, PAGE_SIZE);

        self.start = start;
        self.end = end;
        self.b_pos = start;
        self.p_pos = end;
        self.byte_alloc_count = 0;
    }

    fn add_memory(&mut self, start: usize, size: usize) -> AllocResult {
        Err(AllocError::NoMemory) // unsupported
    }
}

impl<const PAGE_SIZE: usize> ByteAllocator for EarlyAllocator<PAGE_SIZE> {
    fn alloc(&mut self, layout: Layout) -> AllocResult<NonNull<u8>> {
        if layout.size() == 0 {
            return Ok(NonNull::dangling());
        }
        if !layout.align().is_power_of_two() {
            return Err(AllocError::InvalidParam);
        }
        let addr = align_up(self.b_pos, layout.align());
        let new_b = addr
            .checked_add(layout.size())
            .ok_or(AllocError::InvalidParam)?;
        if new_b > self.p_pos {
            return Err(AllocError::NoMemory);
        }
        self.b_pos = new_b;
        self.byte_alloc_count += 1;

        Ok(NonNull::new(addr as *mut u8).unwrap())
    }

    fn dealloc(&mut self, _pos: NonNull<u8>, layout: Layout) {
        if layout.size() == 0 {
            return;
        }
        debug_assert!(self.byte_alloc_count > 0);
        self.byte_alloc_count -= 1;
        if self.byte_alloc_count == 0 {
            self.b_pos = self.start;
        }
    }

    fn total_bytes(&self) -> usize {
       self.end - self.start
    }

    fn used_bytes(&self) -> usize {
        self.b_pos - self.start
    }

    fn available_bytes(&self) -> usize {
        self.p_pos - self.b_pos
    }
}

impl<const PAGE_SIZE: usize> PageAllocator for EarlyAllocator<PAGE_SIZE> {
    const PAGE_SIZE: usize = PAGE_SIZE;

    fn alloc_pages(&mut self, num_pages: usize, align_pow2: usize) -> AllocResult<usize> {
        if num_pages == 0 {
            return Err(AllocError::InvalidParam);
        }
        if align_pow2 % PAGE_SIZE != 0 {
            return Err(AllocError::InvalidParam);
        }
        if !align_pow2.is_power_of_two() {
            return Err(AllocError::InvalidParam);
        }
        let size_bytes = num_pages
            .checked_mul(PAGE_SIZE)
            .ok_or(AllocError::InvalidParam)?;
        if size_bytes > self.p_pos.saturating_sub(self.b_pos) {
            return Err(AllocError::NoMemory);
        }
        let candidate_lo = self.p_pos - size_bytes;
        let aligned_lo = align_down(candidate_lo, align_pow2);
        if aligned_lo < self.b_pos {
            return Err(AllocError::NoMemory);
        }
        if aligned_lo.checked_add(size_bytes).ok_or(AllocError::InvalidParam)? > self.p_pos {
            return Err(AllocError::NoMemory);
        }
        self.p_pos = aligned_lo;
        Ok(aligned_lo)
    }

    fn dealloc_pages(&mut self, _pos: usize, _num_pages: usize) {
        // Page side is never reclaimed in this early bump allocator.
    }

    fn total_pages(&self) -> usize {
        (self.end - self.start) / PAGE_SIZE
    }

    fn used_pages(&self) -> usize {
        (self.end - self.p_pos) / PAGE_SIZE
    }

    fn available_pages(&self) -> usize {
        (self.p_pos - self.b_pos) / PAGE_SIZE
    }
}

//! # Free-List Allocator
//!
//! Building on the bump allocator, implement a Free-List Allocator that supports memory reclamation.
//!
//! ## How It Works
//!
//! A Free-List Allocator uses a linked list to track all freed memory blocks.
//! On allocation, it first searches the list for a suitable block (first-fit strategy);
//! if none is found, it falls back to allocating from the unused region.
//! On deallocation, the block is inserted at the head of the list.
//!
//! ```text
//! free_list -> [block A: 64B] -> [block B: 128B] -> [block C: 32B] -> null
//! ```
//!
//! Each free block stores a `FreeBlock` struct at its head (containing block size and next pointer).
//!
//! ## Task
//!
//! Implement `FreeListAllocator`'s `alloc` and `dealloc` methods:
//!
//! ### alloc
//! 1. Traverse the free_list, find the first block with `size >= layout.size()` and proper alignment (first-fit)
//! 2. If found, remove it from the list and return it
//! 3. If not found, allocate from the `bump` region (same as bump allocator)
//!
//! ### dealloc
//! 1. Write `FreeBlock` header info at the freed block
//! 2. Insert it at the head of free_list
//!
//! ## Key Concepts
//!
//! - Intrusive linked list
//! - `*mut T` read/write: `ptr.write(val)` / `ptr.read()`
//! - Memory alignment checks

#![cfg_attr(not(test), no_std)]

use core::alloc::{GlobalAlloc, Layout};
use core::ptr::null_mut;
use core::sync::atomic::{AtomicBool, Ordering};

struct FreeListGuard<'a>(&'a AtomicBool);

impl Drop for FreeListGuard<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

/// Free block header, stored at the beginning of each free memory block
struct FreeBlock {
    size: usize,
    next: *mut FreeBlock,
}

pub struct FreeListAllocator {
    heap_start: usize,
    heap_end: usize,
    /// Bump pointer: unallocated region starts here
    bump_next: core::sync::atomic::AtomicUsize,
    free_list_lock: AtomicBool,
    /// Free list head (protected by Mutex in test, UnsafeCell otherwise)
    #[cfg(test)]
    free_list: std::sync::Mutex<*mut FreeBlock>,
    #[cfg(not(test))]
    free_list: core::cell::UnsafeCell<*mut FreeBlock>,
}

#[cfg(test)]
unsafe impl Send for FreeListAllocator {}
#[cfg(test)]
unsafe impl Sync for FreeListAllocator {}
#[cfg(not(test))]
unsafe impl Send for FreeListAllocator {}
#[cfg(not(test))]
unsafe impl Sync for FreeListAllocator {}

impl FreeListAllocator {
    /// # Safety
    /// `heap_start..heap_end` must be a valid readable and writable memory region.
    pub unsafe fn new(heap_start: usize, heap_end: usize) -> Self {
        Self {
            heap_start,
            heap_end,
            bump_next: core::sync::atomic::AtomicUsize::new(heap_start),
            free_list_lock: AtomicBool::new(false),
            #[cfg(test)]
            free_list: std::sync::Mutex::new(null_mut()),
            #[cfg(not(test))]
            free_list: core::cell::UnsafeCell::new(null_mut()),
        }
    }

    fn lock(&self) -> FreeListGuard<'_> {
        while self
            .free_list_lock
            .compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            core::hint::spin_loop();
        }
        FreeListGuard(&self.free_list_lock)
    }

    #[cfg(test)]
    fn free_list_head(&self) -> *mut FreeBlock {
        *self.free_list.lock().unwrap()
    }

    #[cfg(test)]
    fn set_free_list_head(&self, head: *mut FreeBlock) {
        *self.free_list.lock().unwrap() = head;
    }

    #[cfg(not(test))]
    fn free_list_head(&self) -> *mut FreeBlock {
        unsafe { *self.free_list.get() }
    }

    #[cfg(not(test))]
    fn set_free_list_head(&self, head: *mut FreeBlock) {
        unsafe { *self.free_list.get() = head }
    }
}

unsafe impl GlobalAlloc for FreeListAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let _guard = self.lock();
        // Ensure block is at least large enough to hold a FreeBlock header (for future dealloc)
        let size = layout.size().max(core::mem::size_of::<FreeBlock>());
        let align = layout.align().max(core::mem::align_of::<FreeBlock>());

        // TODO: Step 1 — traverse free_list, find a suitable block (first-fit)
        //
        // Hints:
        // - Use prev_ptr and curr to traverse the list
        // - Check if curr address satisfies align, and (*curr).size >= size
        // - If found, remove it from the list (update prev's next or the free_list head)
        // - Return curr as *mut u8

        // TODO: Step 2 — no suitable block in free_list, allocate from bump region
        //
        // Same logic as 02_bump_allocator's alloc
        let mut previous: *mut FreeBlock = null_mut();
        let mut current = self.free_list_head();
        while !current.is_null() {
            if current as usize % align == 0 && (*current).size >= size {
                if previous.is_null() {
                    self.set_free_list_head((*current).next);
                } else {
                    (*previous).next = (*current).next;
                }
                return current.cast();
            }
            previous = current;
            current = (*current).next;
        }
        let next = self.bump_next.load(Ordering::Relaxed);
        let Some(aligned) = next
            .checked_add(align - 1)
            .map(|address| address & !(align - 1))
        else {
            return null_mut();
        };
        let Some(end) = aligned.checked_add(size) else {
            return null_mut();
        };
        if aligned < self.heap_start || end > self.heap_end {
            return null_mut();
        }
        self.bump_next.store(end, Ordering::Relaxed);
        aligned as *mut u8
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        let _guard = self.lock();
        let size = layout.size().max(core::mem::size_of::<FreeBlock>());

        // TODO: Insert the freed block at the head of free_list
        //
        // Steps:
        // 1. Cast ptr to *mut FreeBlock
        // 2. Write FreeBlock { size, next: current list head }
        // 3. Update free_list head to ptr
        let block = ptr.cast::<FreeBlock>();
        block.write(FreeBlock {
            size,
            next: self.free_list_head(),
        });
        self.set_free_list_head(block);
    }
}

// ============================================================
// Tests
// ============================================================
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_first_fit_skips_small_block() {
        let (allocator, _heap) = make_allocator();
        let small = Layout::from_size_align(32, 8).unwrap();
        let large = Layout::from_size_align(128, 8).unwrap();
        unsafe {
            let large_pointer = allocator.alloc(large);
            let small_pointer = allocator.alloc(small);
            assert!(!large_pointer.is_null() && !small_pointer.is_null());
            allocator.dealloc(large_pointer, large);
            allocator.dealloc(small_pointer, small);
            assert_eq!(allocator.alloc(large), large_pointer);
            assert_eq!(allocator.alloc(small), small_pointer);
        }
    }

    #[test]
    fn test_concurrent_alloc_and_dealloc() {
        let (allocator, _heap) = make_allocator();
        let layout = Layout::from_size_align(32, 8).unwrap();
        std::thread::scope(|scope| {
            for worker in 0..8 {
                let allocator = &allocator;
                scope.spawn(move || {
                    for _ in 0..1000 {
                        unsafe {
                            let pointer = allocator.alloc(layout);
                            assert!(!pointer.is_null());
                            pointer.write_bytes(worker, layout.size());
                            std::thread::yield_now();
                            for offset in 0..layout.size() {
                                assert_eq!(pointer.add(offset).read(), worker);
                            }
                            allocator.dealloc(pointer, layout);
                        }
                    }
                });
            }
        });
    }

    const HEAP_SIZE: usize = 4096;

    fn make_allocator() -> (FreeListAllocator, Vec<u8>) {
        let mut heap = vec![0u8; HEAP_SIZE];
        let start = heap.as_mut_ptr() as usize;
        let alloc = unsafe { FreeListAllocator::new(start, start + HEAP_SIZE) };
        (alloc, heap)
    }

    #[test]
    fn test_alloc_basic() {
        let (alloc, _heap) = make_allocator();
        let layout = Layout::from_size_align(32, 8).unwrap();
        let ptr = unsafe { alloc.alloc(layout) };
        assert!(!ptr.is_null());
    }

    #[test]
    fn test_alloc_alignment() {
        let (alloc, _heap) = make_allocator();
        for align in [1, 2, 4, 8, 16] {
            let layout = Layout::from_size_align(8, align).unwrap();
            let ptr = unsafe { alloc.alloc(layout) };
            assert!(!ptr.is_null());
            assert_eq!(ptr as usize % align, 0, "align={align}");
        }
    }

    #[test]
    fn test_dealloc_and_reuse() {
        let (alloc, _heap) = make_allocator();
        let layout = Layout::from_size_align(64, 8).unwrap();

        let p1 = unsafe { alloc.alloc(layout) };
        assert!(!p1.is_null());

        // After freeing, the next allocation should reuse the same block
        unsafe { alloc.dealloc(p1, layout) };
        let p2 = unsafe { alloc.alloc(layout) };
        assert!(!p2.is_null());
        assert_eq!(p1, p2, "should reuse the freed block");
    }

    #[test]
    fn test_multiple_alloc_dealloc() {
        let (alloc, _heap) = make_allocator();
        let layout = Layout::from_size_align(128, 8).unwrap();

        let p1 = unsafe { alloc.alloc(layout) };
        let p2 = unsafe { alloc.alloc(layout) };
        let p3 = unsafe { alloc.alloc(layout) };
        assert!(!p1.is_null() && !p2.is_null() && !p3.is_null());

        unsafe { alloc.dealloc(p2, layout) };
        unsafe { alloc.dealloc(p1, layout) };

        let q1 = unsafe { alloc.alloc(layout) };
        let q2 = unsafe { alloc.alloc(layout) };
        assert!(!q1.is_null() && !q2.is_null());
    }

    #[test]
    fn test_oom() {
        let (alloc, _heap) = make_allocator();
        let layout = Layout::from_size_align(HEAP_SIZE + 1, 1).unwrap();
        let ptr = unsafe { alloc.alloc(layout) };
        assert!(ptr.is_null(), "should return null when exceeding heap");
    }
}

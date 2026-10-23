//! Measures allocations during predeserialized, stable-size updates.

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

use shuai::game::GameState;
use shuai_client::server::event::{GameStartData, GameUpdateData};

struct CountingAllocator;
static ALLOCATIONS: AtomicUsize = AtomicUsize::new(0);

// SAFETY: Every request is forwarded unchanged to System with its original layout.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        // SAFETY: The caller supplied the allocation layout required by System.
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        // SAFETY: The pointer and its original layout are forwarded to System.
        unsafe { System.dealloc(pointer, layout) }
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        // SAFETY: The caller's original allocation and new size are preserved.
        unsafe { System.realloc(pointer, layout, size) }
    }
}

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

fn update(map_diff: Vec<i64>) -> GameUpdateData {
    GameUpdateData {
        turn: 1,
        scores: Vec::new(),
        attack_index: Some(-1),
        generals: Vec::new(),
        map_diff,
        cities_diff: Vec::new(),
        deserts_diff: None,
        stars: None,
        uncertainties: None,
        alt_stars: None,
        raw_visible: None,
        pings: None,
        chess_clock: None,
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let metadata: GameStartData = serde_json::from_str(
        r#"{
        "playerIndex":0,"chat_room":"offline","usernames":["local"],
        "game_type":"custom","options":{}
    }"#,
    )?;
    let mut state = GameState::default();
    state.start(Box::new(metadata))?;
    state.update(update(vec![0, 10, 2, 2, 0, 1, 2, 3, -1, -1, -1, 0]))?;
    state.update(update(vec![10]))?;
    let updates: Vec<_> = (0..10_000).map(|_| update(vec![2, 1, 8, 7])).collect();
    let before = ALLOCATIONS.load(Ordering::Relaxed);
    let started = Instant::now();
    for event in updates {
        state.update(event)?;
    }
    let elapsed = started.elapsed();
    let allocations = ALLOCATIONS.load(Ordering::Relaxed) - before;
    println!("10000 updates: {allocations} allocation/reallocation calls, {elapsed:?}");
    Ok(())
}

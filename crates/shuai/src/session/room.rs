const RANDOM_ROOM_ID_LENGTH: usize = 4;

/// Which queue Session joins, and how a private room ID is chosen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GameMode {
    /// A private room whose ID persists across requeues.
    FixedPrivate(String),
    /// A private room with a new random ID on each requeue. An accepted rematch
    /// stays in the same room because it does not requeue.
    RandomPrivate,
    /// The public 1v1 matchmaking queue.
    OneVsOne,
}

pub(super) fn random_room_id() -> String {
    std::iter::repeat_with(fastrand::alphanumeric)
        .take(RANDOM_ROOM_ID_LENGTH)
        .collect()
}

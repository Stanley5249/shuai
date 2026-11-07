const RANDOM_ROOM_ID_LENGTH: usize = 4;

/// Which queue Session joins, and how a private room ID is chosen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GameMode {
    /// A private room rejoined after every match, so its players can play again.
    FixedPrivate(String),
    /// A new private room with a random ID for every match.
    RandomPrivate,
    /// The public 1v1 matchmaking queue, the one mode that votes for rematches.
    OneVsOne,
}

pub(super) fn random_room_id() -> String {
    std::iter::repeat_with(fastrand::alphanumeric)
        .take(RANDOM_ROOM_ID_LENGTH)
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Activity {
    Coffee,
    Dinner,
    Drinks,
    GameNight,
    Virtual,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mood {
    Relaxed,
    Energetic,
    CasualCatchUp,
    Celebratory,
    Brainstorming,
}

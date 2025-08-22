#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Activity {
    Coffee,
    Lunch,
    Drinks,
    Dinner,
    Active, // Walk, bowling, games, etc.
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mood {
    Quick,     // 30 mins max
    Chill,     // Low key, no pressure
    Energetic, // High energy, ready to do things
    Quiet,     // Intimate conversations, low noise
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GroupSize {
    OneOnOne, // 1-on-1 only
    Small,    // 3-5 people
    Large,    // 6+ people
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct What {
    pub activity: Option<Activity>,
    pub mood: Option<Mood>,
    pub group_size: Option<GroupSize>,
}

impl What {
    pub fn new() -> Self {
        Self {
            activity: None,
            mood: None,
            group_size: None,
        }
    }

    pub fn with_activity(mut self, activity: Activity) -> Self {
        self.activity = Some(activity);
        self
    }

    pub fn with_mood(mut self, mood: Mood) -> Self {
        self.mood = Some(mood);
        self
    }

    pub fn with_group_size(mut self, group_size: GroupSize) -> Self {
        self.group_size = Some(group_size);
        self
    }
}

impl Default for What {
    fn default() -> Self {
        Self::new()
    }
}

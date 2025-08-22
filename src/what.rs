#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Activity {
    Coffee,
    Lunch,
    Drinks,
    Dinner,
    BoardGame,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mood {
    Quick,     // 30 mins max
    Chill,     // Low key, no pressure
    Energetic, // TODO: need a better word to convey something public, busy place or just need to be around some people energy.
    Quiet,     // Intimate conversations, low noise
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GroupSize {
    // TODO: I think the min/max should be associated data for clear rule matching, e.g.
    // `OneOnOne((u8, u8))` or maybe something like `OneOnOne(GroupLimits); struct GroupLimits {min: Option<u8>, max: Option<u8>,}`
    OneOnOne, // 1-on-1 only
    Small,    // upto 6, at least 2, including organizer
    Large,    // upto 12, at least 4, including organizer
    Party,    // no max, at least 6, including organizer
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

// TODO: Does this module need tests? Feel like no, but, thoughts?

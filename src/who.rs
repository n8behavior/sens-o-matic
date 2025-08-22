#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Participant {
    pub name: String,
    pub email: String,
}

impl Participant {
    pub fn new(name: impl Into<String>, email: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            email: email.into(),
        }
    }
}

#[derive(Clone)]
pub struct Gathering {
    pub recipients: Vec<Participant>,
    pub interested: Vec<Participant>,
    pub unavailable: Vec<Participant>,
}

impl Gathering {
    pub fn new(recipients: Vec<Participant>) -> Self {
        Gathering {
            recipients: recipients.clone(),
            interested: Vec::new(),
            unavailable: Vec::new(),
        }
    }

    pub fn pending(&self) -> Vec<Participant> {
        self.recipients
            .iter()
            .filter(|p| !self.interested.contains(p) && !self.unavailable.contains(p))
            .cloned()
            .collect()
    }

    pub fn all_responded(&self) -> bool {
        self.pending().is_empty()
    }

    pub fn has_interested_participants(&self) -> bool {
        !self.interested.is_empty()
    }

    pub fn should_fail_intent(&self) -> bool {
        self.interested.is_empty() && self.all_responded()
    }

    pub fn apply_timeout(&mut self) {
        let pending = self.pending();
        self.unavailable.extend(pending);
    }
}

# Sens-O-Matic

> "The history of every major Galactic civilization tends to pass through three distinct and recognizable phases, those of Survival, Inquiry, and Sophistication, otherwise known as the How, Why, and Where phases. For instance, the first phase is characterized by the question 'How can we eat?', the second by the question 'Why do we eat?', and the third by the question 'Where shall we have lunch?'"
>
> This app solves the fourth phase: "Who's free for drinks after work?"

## Vocabulary

### Core Concepts

**Cohort** - The complete set of all registered users in a friend group or community. These are all potential participants in any gathering, whether currently online or offline.

**Members** - Individual users within a cohort. Each member has an identity (name, email) and various states throughout the coordination process.

**Recipients** - The subset of the cohort that receives a ping for a specific gathering. This could be everyone currently online, a curated list, or based on preferences.

**Respondents** - Recipients who have provided any response during the Gathering phase (either interested or unavailable). Those who haven't responded are considered "pending."

**Participants** - Members who are actively participating in the current phase of coordination:

- In Gathering: those marked as "interested"
- In When: those negotiating time
- In Where: those voting on location
- In RSVP: those confirming attendance

**Attendees** - The final set of members who have confirmed their attendance through the RSVP phase.

### The Winnowing Process

Each phase acts as a filter, progressively narrowing down from the full cohort to actual attendees:

1. **Cohort** → (filter by availability/online) → **Recipients** (Pinging phase)
2. **Recipients** → (filter by interest) → **Participants** (Gathering phase)
3. **Participants** → (filter by time availability) → **Available** (When phase)
4. **Available** → (filter by location preference) → **Voters** (Where phase)
5. **Voters** → (filter by RSVP confirmation) → **Attendees** (RSVP phase)

### Member States by Phase

Throughout the coordination flow, each member has specific states:

- **Pinging**: `unreachable` | `reached`
- **Gathering**: `pending` | `interested` | `unavailable`
- **When**: `proposing` | `agreed` | `conflicted`
- **Where**: `voted` | `abstained`
- **RSVP**: `confirmed` | `declined` | `tentative`

## Don't Panic

Planning social gatherings with friends shouldn't be harder than calculating the probability of being rescued from deep space (which, for the record, is 2^276,709 to 1 against). Yet somehow, the simple act of coordinating "let's grab drinks" often involves more back-and-forth messages than a Vogon bureaucracy permit application.

## The Answer to Life, the Universe, and Getting Together

Sens-O-Matic is a social planning library that makes spontaneous hangouts as easy as falling off a log. Or more accurately, as easy as falling through space (which, as any hitchhiker knows, is remarkably easy - the trick is learning how to miss the ground).

The entire process requires just a few button clicks. No committees. No endless group chats. No planning paralysis. Just pure, distilled social coordination.

## The Journey Through Hyperspace (How It Works)

### Phase 1: Wanting

_Initial state - Like Arthur Dent wanting a proper cup of tea_

Bob feels like grabbing a drink. He opens the app, which starts in the "Wanting" state. He checks a few boxes about what he's in the mood for (drinks, dinner, games) or selects "up for anything" - the pangalactic gargleblaster of social options.

### Phase 2: Pinging

_Broadcasting across the sub-ether network_

Bob sends out a ping to selected recipients from his cohort. Recipients can be chosen through various strategies:
- **All Online** - Broadcast to everyone currently available
- **Favorites** - Start with closest friends first  
- **Group Size Matching** - If Bob wants a small gathering, ping fewer people
- **Cascading** - Start small, expand if insufficient responses

The message travels faster than bad news through the cosmos, reaching all selected members who are online or have notifications enabled. The pinging can be immediate (everyone at once) or sequenced (waves of invites based on response rates).

### Phase 3: Gathering

_The Vogon Poetry Reading Period (but mercifully shorter)_

Recipients enter a brief response period where they indicate interest. Think of it as the opposite of a Vogon poetry reading - people actually want to participate. Recipients who don't respond within the timeout are marked as unavailable (they're probably stuck in a bypass planning meeting). Those who express interest become participants for the next phases.

### Phase 4: When (Temporal Coordination)

_Easier than time travel, harder than missing the ground_

The app presents participants with Bob's initial time preference. Participants can agree or propose alternatives. After another brief settling period, the system employs advanced strategies:

- "Closest to mine" - For the temporally selfish
- "Most popular" - Democracy in action
- "Must include Ford and Zaphod" - For when specific participants are essential

Bob picks the winning time. The Infinite Improbability Drive has nothing on this level of coordination.

### Phase 5: Where (Location Triangulation)

_Now with 42% less confusion_

Bob gets first dibs on location selection, presented with intelligent strategies:

- "Closest to me" - The Zaphod Beeblebrox approach
- "Geographic center" - Fair and balanced, like a properly toweled hitchhiker
- "Has Pan-Galactic Gargle Blasters" - Feature-based selection
- "Not in Vogon territory" - Avoidance zones

Bob can choose a strategy or open it up to the group. The strategies generate candidate locations, building a list of possibilities (top 10, because even Deep Thought needs limits).

### Phase 6: Voting

_More democratic than the Galactic Government_

Available participants engage in ranked choice voting for the location list generated in Phase 5. The app handles tie-breaking automatically after a timeout. No need for Marvin's depressing input or a committee forming a committee to decide how to form a committee. This sophisticated voting system works better than any Galactic presidential election.

### Phase 7: RSVP

_Final boarding call for the Heart of Gold_

The final What/Where/When is sent as an invite to all voters. Options include minimum attendance requirements - if fewer than N attendees confirm, the intent fails and everyone tries again another time (probably when Earth hasn't been demolished for a hyperspace bypass).

## Key Features (Mostly Harmless)

- **Babel Fish Simplicity**: Universally understandable interface
- **Deep Thought Strategies**: Intelligent decision algorithms for every phase
- **Marvin-Proof**: Designed to work even when everything seems pointless
- **Towel Compatible**: Always know where your towel is (location services)
- **Bypass Resistant**: No committee can stop a plan once in motion

## Future Enhancements (Currently at Marklar IV)

- Late joining for fashionably late arrivals
- Multi-group coordination (for when your different circles overlap like badly drawn Venn diagrams)
- Historic pattern learning (the app remembers that Trillian always votes for wine bars)

## Technical Guide for Hitchhikers

### Building the Probability Drive

```bash
cargo build           # Construct the Heart of Gold
cargo build --release # With infinite improbability optimization
```

### Testing the Sub-Ether Circuits

```bash
cargo test                    # Test all components
cargo test test_name          # Test specific component
cargo test -- --nocapture     # See the dolphins' goodbye messages
```

### Formatting and Linting (Keeping it Zarquon Clean)

```bash
cargo fmt              # Format code to Galactic standards
cargo clippy           # Let the ship's computer critique your code
```

## Architecture (The Encyclopedia Galactica Entry)

The library implements a typestate pattern that would make even Slartibartfast proud. Each state transition is enforced at compile time - preventing impossible states more effectively than the laws of physics prevent faster-than-light travel (which, as we know, they don't really).

### Core Modules

- **`who`**: Participant modeling (currently as empty as space)
- **`what`**: Activities and moods (from Coffee to Pan-Galactic Gargle Blasters)
- **`where`**: Location services (in development, currently lost like Zaphod's second head)
- **`when`**: Temporal coordination (handles everything except actual time travel)

## Dependencies

- **chrono**: For time handling (because even in an infinite universe, timing matters)

## Contributing

If you'd like to help make spontaneous social planning a reality, grab your towel and join us. We promise the code reviews are nothing like Vogon poetry readings.

## License

This project is about as free as a hitchhiker floating through space - which is to say, very free indeed, within the constraints of whatever license we eventually choose.

---

_Remember: The secret to spontaneous social success is knowing where your friends are, when they're free, and always carrying a towel._

_So long, and thanks for all the pull requests._


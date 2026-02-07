# Sens-O-Matic Application Specification

## 1. Overview & Problem Statement

### The Problem

Getting friends together for spontaneous hangouts is harder than it should be. The typical group chat approach generates dozens of messages as people negotiate timing and location:

- "I'm free but not if it's downtown"
- "I can make it but only after 5:30"
- "Is anyone still going to be there at 7?"
- "Wait, where are we going again?"
- "Actually I can't do that place"

This coordination friction kills spontaneity. By the time everyone agrees, the window has passed or people have lost interest.

### The Solution

Sens-O-Matic streamlines group coordination by:

1. **Collecting structured availability** - Instead of free-form chat, capture when people can be there and their constraints
2. **Finding the overlap** - Algorithmically identify time windows and venues that work for the most people
3. **Reducing back-and-forth** - One ping, structured responses, automated matching

### Core Value Proposition

Turn "Who's up for drinks?" into an actual hangout with minimal friction.

### Guiding Principles

1. **Natural language first** - Voice-to-text, conversational input ("I'm free after 4:30, can stay a couple hours")
2. **Touch as quick alternative** - Simple yes/no, time sliders for fast responses
3. **Near effortless UX** - Minimize friction at every step
4. **Privacy-first** - Invite codes only, no contact access, minimal data collection
5. **Technology-agnostic** - This spec focuses on features/UX, not implementation details

---

## 2. Core Concepts

### User

A person using the app.

**Properties:**
- `id` - Unique identifier
- `name` - Display name visible to group members
- `avatar` - Profile image (optional)
- `preferences` - Default preferences for hangouts
  - `default_distance` - How far they typically travel
  - `favorite_areas` - Neighborhoods/areas they prefer
  - `home_location` - For ETA calculations (opt-in, encrypted)

**Visibility:** Full profiles visible to group members (name, avatar, preferences).

### Group

A circle of friends who coordinate hangouts together.

**Properties:**
- `id` - Unique identifier
- `name` - Group display name
- `members` - List of users in the group
- `invite_code` - Code for adding new members (can be regenerated)
- `favorites` - Venues the group has saved
- `encryption_keys` - Keys for E2E encryption (distributed to members)

**Rules:**
- Join only via invite code (no contact mining, no discovery)
- Any member can generate a new invite code
- Any member can remove themselves from a group

### Ping

An open invitation to gauge interest in a hangout.

**Properties:**
- `id` - Unique identifier
- `initiator` - User who created the ping
- `group` - Target group
- `activity_type` - What kind of hangout (drinks, dinner, coffee, etc.)
- `rough_timing` - General time frame (today, tonight, this afternoon)
- `vibe` - Optional mood/style hints (chill, celebration, casual)
- `created_at` - When the ping was sent
- `state` - Current state in the lifecycle (see State Machine)
- `responses` - Collected responses from group members

**Concurrency:** Users can participate in multiple active pings simultaneously.

### Response

A group member's answer to a ping.

**Properties:**
- `user` - Who is responding
- `answer` - Yes or No (no maybes)
- `availability` - Time window if Yes
  - `earliest` - Earliest they can arrive
  - `latest` - Latest they can stay until
- `preferences` - Optional constraints
  - `max_distance` - How far they'll travel
  - `preferred_areas` - Where they'd like to go
  - `excluded_areas` - Where they won't go
  - `vibe` - Style preferences
- `updated_at` - Last modification time

**Mutability:** Responses can be changed:
- Yes → No (cancel): Allowed, group is notified
- No → Yes (join late): Allowed if hangout hasn't reached capacity

### Venue

A place where the hangout could occur.

**Properties:**
- `id` - Unique identifier
- `name` - Venue name
- `location` - Address and coordinates
- `category` - Type (bar, restaurant, coffee shop, etc.)
- `vibe_tags` - Style descriptors (chill, upscale, dive, loud, etc.)
- `capacity_hint` - Rough sense of size (small, medium, large)
- `source` - Where this data came from (group favorite, external API)
- `group_notes` - Comments from group members about this venue

**Sources:**
- **Group favorites** - Venues explicitly saved by the group
- **Discovery** - Venues from external APIs (implementation-defined)

### Hangout

A realized event once a ping has been matched to a venue.

**Properties:**
- `id` - Unique identifier
- `ping` - The originating ping
- `venue` - The selected venue
- `confirmed_attendees` - Users who are coming
- `timeline` - Who's arriving/leaving when
- `status` - Current hangout status
  - `confirmed` - Venue selected, waiting for start
  - `active` - Hangout is happening
  - `complete` - Hangout has ended
- `attendee_states` - Real-time status of each attendee (opt-in)

---

## 3. The Flow (State Machine)

```
┌──────────────────────────────────────────────────────────────────────────┐
│                         PING LIFECYCLE                                    │
└──────────────────────────────────────────────────────────────────────────┘

     ┌─────┐
     │Idle │ ─── User creates ping ───▶ ┌─────────────┐
     └─────┘                            │ Ping Sent   │
        ▲                               └──────┬──────┘
        │                                      │
        │                          Responses arrive
        │                                      │
        │                                      ▼
        │                             ┌─────────────────┐
        │                             │    Gathering    │
        │                             │    Responses    │
        │                             └────────┬────────┘
        │                                      │
        │              Timeout/threshold reached OR initiator triggers
        │                                      │
        │                                      ▼
        │                              ┌───────────────┐
        │                              │   Matching    │
        │                              └───────┬───────┘
        │                                      │
        │            ┌─────────────────────────┼─────────────────────────┐
        │            │                         │                         │
        │            ▼                         ▼                         ▼
        │    ┌──────────────┐         ┌──────────────┐          ┌──────────────┐
        │    │  No Match    │         │ Venue Vote   │          │ Auto-Select  │
        │    │  (0 yes)     │         │   Active     │          │ (threshold)  │
        │    └──────┬───────┘         └──────┬───────┘          └──────┬───────┘
        │           │                        │                         │
        │           │                  Voting complete                 │
        │           │                        │                         │
        │           │                        ▼                         │
        │           │                ┌──────────────┐                  │
        │           │                │   Venue      │◀─────────────────┘
        │           │                │  Confirmed   │
        │           │                └──────┬───────┘
        │           │                       │
        │           │               Start time reached
        │           │                       │
        │           │                       ▼
        │           │               ┌──────────────┐
        │           │               │    Active    │
        │           │               │   Hangout    │
        │           │               └──────┬───────┘
        │           │                      │
        │           │              All attendees left OR timeout
        │           │                      │
        │           ▼                      ▼
        │   ┌──────────────┐       ┌──────────────┐
        └───│  Cancelled   │       │   Complete   │
            └──────────────┘       └──────────────┘
```

### State Descriptions

#### Idle
- No active ping for this user in this group
- User can create a new ping or respond to others

#### Ping Sent
- Ping has been created and broadcast to group
- Waiting for first response
- Transitions to Gathering Responses on first response
- Can be cancelled by initiator

#### Gathering Responses
- Collecting yes/no answers with availability
- New responses update the pool
- Existing responses can be modified
- Transitions to Matching when:
  - Gathering timeout reached (configurable, e.g., 30 mins)
  - Minimum response threshold met AND initiator triggers
  - All group members have responded

#### Matching
- Algorithm identifies:
  - Time overlap across "yes" respondents
  - Venues matching collective preferences
- If no overlap or no "yes" responses → No Match
- If matches found → Generate ranked venue options

#### Venue Vote Active
- Group sees ranked venue options
- Each attendee votes on preferences
- Voting has timeout (e.g., 10 mins)
- Highest-voted venue wins
- Ties broken by: initiator preference > first response > random

#### Venue Confirmed
- Location and time are set
- Attendees receive confirmation
- Attendees can still cancel (with notification to group)
- Late joiners can request to join

#### Active Hangout
- Event is happening
- Location sharing enabled (opt-in)
- Attendee status updates (enroute, arrived, left)
- New arrivals welcomed, departures noted

#### Complete
- Hangout has ended
- Final state for successful pings
- Venue can be saved to group favorites

#### Cancelled / No Match
- Terminal states
- No match: zero "yes" responses or no viable overlap
- Cancelled: initiator explicitly cancelled

### Edge Case Transitions

| From State | Event | To State |
|------------|-------|----------|
| Gathering | All users respond "no" | No Match |
| Gathering | Initiator cancels | Cancelled |
| Venue Vote | All voters cancel | Cancelled |
| Venue Confirmed | All attendees cancel | Cancelled |
| Active Hangout | All attendees leave | Complete |

---

## 4. User Flows

### 4.1 Onboarding

#### First Launch

**Screen: Welcome**
- App name and tagline
- "Get Started" button

**Screen: Create Identity**
- Input: Name (required)
- Input: Avatar (optional, can use camera or gallery)
- Touch: "Continue" button

**Screen: Join Your First Group**
- Explanation: "Sens-O-Matic uses invite codes. Ask a friend for their group's code."
- Input field: Invite code (6-8 alphanumeric characters)
- Touch: "Join Group" button
- Alternative: "Create New Group" link

**Screen: Create New Group (if chosen)**
- Input: Group name
- Touch: "Create" button
- Result: Shows invite code to share with friends

**Screen: Ready**
- Confirmation of group joined/created
- Brief tutorial overlay (dismissible)
- Transition to main screen

### 4.2 Creating a Ping

#### Natural Language Path

**Screen: Main / Group View**
- Shows current group and any active pings
- Large input area (voice or text)
- Microphone icon for voice input

**Input Examples:**
- "Who's up for drinks today?"
- "Anyone want to grab dinner tonight? Thinking somewhere chill"
- "Coffee this afternoon?"
- "Beers after work, nothing fancy"

**NLU Extraction:**
| Input | Activity | Timing | Vibe |
|-------|----------|--------|------|
| "drinks today" | drinks | today (general) | - |
| "dinner tonight, somewhere chill" | dinner | tonight | chill |
| "coffee this afternoon" | coffee | afternoon | - |
| "beers after work, nothing fancy" | drinks | after work (~5-6pm) | casual |

**Screen: Confirm Ping**
- Shows extracted details:
  - Activity: drinks
  - Timing: today
  - Vibe: chill (if detected)
- Touch: "Send Ping" / "Edit" buttons
- Edit allows corrections before sending

#### Touch-Only Path

**Screen: Main / Group View**
- "New Ping" button (as alternative to NL input)

**Screen: Create Ping (Structured)**
- Picker: Activity type (drinks, dinner, coffee, activity, other)
- Picker: When (now, this afternoon, tonight, tomorrow)
- Optional: Vibe tags (chill, upbeat, fancy, dive, outdoors)
- Touch: "Send Ping" button

### 4.3 Responding to a Ping

#### Notification

**Push Notification:**
- "[Name] pinged [Group]: Who's up for drinks today?"
- Tap opens app to ping response screen

#### Natural Language Path

**Screen: Ping Response**
- Shows ping details (who, what, when general)
- Large input area for response

**Input Examples:**
- "Sounds good, free after 4:30, can stay a couple hours"
- "I'm in! Available from 5 to 8"
- "Yes but not downtown"
- "Can't today"
- "No"

**NLU Extraction:**
| Input | Answer | Availability | Constraints |
|-------|--------|--------------|-------------|
| "free after 4:30, couple hours" | yes | 4:30pm - 6:30pm | - |
| "Available from 5 to 8" | yes | 5:00pm - 8:00pm | - |
| "Yes but not downtown" | yes | (needs follow-up) | exclude: downtown |
| "Can't today" | no | - | - |

**Ambiguity Handling:**
If availability is unclear (e.g., "Yes but not downtown"):
- System: "When are you available?"
- User can respond naturally or tap time picker

**Screen: Confirm Response**
- Shows interpreted response
- Visual timeline showing your availability
- Touch: "Send" / "Edit" buttons

#### Touch-Only Path

**Screen: Ping Response**
- Large "Yes" / "No" buttons at top

**If Yes tapped → Screen: Set Availability**
- Time slider or picker:
  - "Earliest arrival" (scrollable time)
  - "Latest departure" (scrollable time)
- Optional: Distance slider ("How far will you travel?")
- Optional: Area exclusions (tap to exclude areas)
- Touch: "Send Response" button

**If No tapped:**
- Immediate send, returns to main screen
- Optional: Quick reason ("busy", "not feeling it", custom)

### 4.4 Changing Your Mind

#### Cancelling (Was Yes)

**Screen: Active Ping View**
- Shows your current response
- "Change Response" button

**Screen: Change Response**
- "I can't make it anymore" / "Cancel my spot" button
- Confirmation: "The group will be notified. Cancel?"
- Touch: "Yes, cancel" / "Never mind"

**Result:**
- Response updated to No
- Group receives notification: "[Name] can no longer make it"
- Matching recalculates if in Gathering phase

#### Joining Late (Was No)

**Screen: Active Ping View** (if ping still active)
- Shows ping you declined
- "I can come after all" button

**Screen: Join Ping**
- Same as normal response flow (availability input)
- Note if venue already selected: "The group is going to [Venue]. Want to join?"

**If Hangout at Capacity:**
- "This hangout is full. Request to join anyway?"
- Initiator/group receives request

### 4.5 The Match Phase

**Trigger:** Gathering phase ends (timeout, threshold, or manual trigger by initiator)

#### Screen: Match Results

**If matches found:**

```
┌─────────────────────────────────────────┐
│  3 venues match your group              │
├─────────────────────────────────────────┤
│                                         │
│  HANGOUT: 4:30pm - 8pm                  │
│  Peak: 5pm - 6:30pm (everyone there)    │
│                                         │
│  TIMELINE                               │
│  ─────────────────────────────────────  │
│  4pm    5pm    6pm    7pm    8pm       │
│  ├──────┼──────┼──────┼──────┤         │
│  Sarah  ████████████                    │
│  Mike        ██████████████             │
│  Jen         ████████                   │
│  You    ████████████████████            │
│  ─────────────────────────────────────  │
│  4:30-5pm: 2 people                     │
│  5-6:30pm: all 4 ★                      │
│  6:30-8pm: 2-3 people                   │
│                                         │
│  VENUE OPTIONS                          │
│  ─────────────────────────────────────  │
│  1. The Rusty Nail  ★★★★ (favorite)    │
│     Dive bar · 0.5 mi · matches: chill  │
│     [ Vote ]                            │
│                                         │
│  2. Murphy's Pub                        │
│     Irish pub · 0.8 mi · good for groups│
│     [ Vote ]                            │
│                                         │
│  3. Sidebar Lounge                      │
│     Cocktail bar · 1.2 mi · upscale     │
│     [ Vote ]                            │
│                                         │
├─────────────────────────────────────────┤
│  [ See all options ]                    │
└─────────────────────────────────────────┘
```

**Timeline Notes:**
- Hangout runs as long as 2+ people are there
- Peak time highlighted (when most people overlap)
- Users can see who they'll catch at different arrival times

**Interaction:**
- Tap venue to see details (photos, notes, map)
- Vote button registers preference
- Can vote for multiple venues (ranked preference)
- See vote counts update in real-time

**Voting Timeout:**
- Timer shown: "Voting closes in 8:42"
- When voting ends, highest-voted venue wins

**Tie Breaking:**
1. Initiator's preference
2. First responder's preference
3. Random selection

#### Screen: No Viable Overlap

If fewer than 2 people can be there at the same time:
```
┌─────────────────────────────────────────┐
│  Can't find a time that works           │
├─────────────────────────────────────────┤
│                                         │
│  No two schedules overlap:              │
│                                         │
│  4pm    5pm    6pm    7pm    8pm       │
│  ├──────┼──────┼──────┼──────┤         │
│  Sarah  ████████                        │
│  Mike             ████████████          │
│                                         │
│  Sarah leaves before Mike arrives.      │
│                                         │
├─────────────────────────────────────────┤
│  [ Try Different Time ]  [ Cancel ]     │
└─────────────────────────────────────────┘
```

### 4.6 Active Hangout

**Trigger:** Venue confirmed → Start time reached (or manually started)

#### Screen: Active Hangout View

```
┌─────────────────────────────────────────┐
│  DRINKS AT THE RUSTY NAIL               │
│  Tonight · 4:30pm - 8pm                 │
├─────────────────────────────────────────┤
│                                         │
│  📍 The Rusty Nail                      │
│  123 Main St · 0.5 mi away              │
│  [ Directions ]                         │
│                                         │
│  ─────────────────────────────────────  │
│  WHO'S COMING                           │
│  ─────────────────────────────────────  │
│                                         │
│  Sarah     🟢 Arrived                   │
│  Mike      🚗 Enroute · 8 min           │
│  You       📍 Hasn't left yet           │
│  Jen       ⏳ Joining at 6pm            │
│                                         │
│  ─────────────────────────────────────  │
│  [ I'm leaving now ]                    │
│  [ Running late ]                       │
│  [ I'm here ]                           │
│  [ Had to leave ]                       │
│                                         │
└─────────────────────────────────────────┘
```

#### Location-Aware Status (Opt-in)

If user has shared home location and granted location permission:

**Automatic States:**
- **Hasn't left yet** - User still at home location
- **Enroute, ~X mins away** - User in transit, live ETA calculation
- **Arrived** - User at or very near venue

**Manual Overrides:**
- "I'm leaving now" - Updates status to enroute (for users without location sharing)
- "Running late" - Flags status, can add ETA
- "I'm here" - Marks as arrived
- "Had to leave" - Removes from active attendees with notification

**Privacy Controls:**
- Location sharing is opt-in at user level
- Can be toggled per-hangout
- Location data encrypted, only visible to group

#### Late Joiners

If someone who said "no" wants to join:
- They see "Join this hangout" option
- Group receives notification: "[Name] wants to join"
- They receive venue details once confirmed

#### Hangout End

**Triggers:**
- All attendees marked as "left"
- Timeout after latest departure time
- Initiator ends hangout manually

**Screen: Hangout Summary**
- Who attended
- Duration
- Option: "Save The Rusty Nail to favorites?"
- Option: "Add note about this venue"

---

## 5. Natural Language Understanding

### Intent Recognition

The NLU system must recognize these primary intents:

| Intent | Example Phrases |
|--------|-----------------|
| `create_ping` | "Who's up for...", "Anyone want to...", "Let's grab..." |
| `respond_yes` | "I'm in", "Sounds good", "Yes", "Count me in", "Free after..." |
| `respond_no` | "Can't", "Not today", "No", "Pass", "Won't make it" |
| `cancel` | "Can't make it anymore", "Have to cancel", "Something came up" |
| `join_late` | "Actually I can come", "Changed my mind", "Room for one more?" |
| `check_status` | "What's happening?", "Where are we going?", "Who's coming?" |
| `update_eta` | "Running late", "Be there in 10", "Leaving now" |
| `arrived` | "I'm here", "Just got here", "At the bar" |
| `leaving` | "Gotta go", "Heading out", "Had to leave" |

### Time Parsing

The system must interpret natural time expressions relative to current time:

| Input | Interpretation (assuming current time 2pm) |
|-------|-------------------------------------------|
| "after 4:30" | earliest: 4:30pm |
| "until 7ish" | latest: ~7:00pm (±15 min tolerance) |
| "couple hours" | duration: ~2 hours |
| "tonight" | 5pm - 11pm (contextual) |
| "this afternoon" | 12pm - 5pm |
| "after work" | 5pm - 6pm (or user's configured end time) |
| "now" | current time |
| "in an hour" | current time + 1 hour |
| "5 to 8" | earliest: 5pm, latest: 8pm |

**Duration to End Time:**
When user specifies duration ("couple hours") plus start ("after 4:30"):
- Calculate: earliest 4:30pm, latest 6:30pm

### Preference Extraction

| Input | Extracted Preference |
|-------|---------------------|
| "somewhere chill" | vibe: chill |
| "not downtown" | exclude_area: downtown |
| "walking distance" | max_distance: 0.5 mi (configurable) |
| "nothing fancy" | vibe: casual, exclude_vibe: upscale |
| "outdoor seating" | feature: outdoor |
| "not too loud" | vibe: quiet |

### Ambiguity Handling

When input is ambiguous, the system should:

1. **Ask for clarification** with natural prompts:
   - Ambiguous: "I'm in"
   - System: "Great! When can you be there?"

2. **Offer structured fallback:**
   - "I couldn't quite parse that. Mind using the time picker?"

3. **Confirm interpretations:**
   - User: "Free from like 5ish to whenever"
   - System: "Got it - available 5pm onwards. Correct?"

### Multi-Turn Context

The NLU should maintain context across turns:

```
User: "Sounds good"
System: "When are you available?"
User: "After 5"
System: "How long can you stay?"
User: "Couple hours"
→ Response: yes, 5:00pm - 7:00pm
```

---

## 6. Place/Venue System

### Data Sources

#### Group Favorites

Venues explicitly saved by the group:
- Saved after successful hangouts
- Manually added by group members
- Include group-specific notes ("great happy hour", "ask for back room")

**Favorite Properties:**
- All standard venue properties
- `times_visited` - How often the group has been here
- `last_visited` - Most recent hangout here
- `group_rating` - Average group rating
- `group_notes` - Comments from group members

#### Discovery (External APIs)

New venues sourced from external services:
- Implementation defines specific APIs (Yelp, Google Places, Foursquare, etc.)
- Filtered by group's collective constraints

**Discovery Filters:**
- Location (centroid of attendees or specified area)
- Category (bar, restaurant, coffee shop)
- Distance from each attendee
- Capacity (based on group size)
- Vibe tags if available

### Venue Matching

When matching phase begins, score venues by:

1. **Location Score**
   - Calculate distance from each attendee
   - Penalize if exceeds any attendee's max_distance
   - Bonus for central location

2. **Preference Score**
   - Match vibe tags to group preferences
   - Penalize if in any attendee's excluded_area
   - Bonus for matching positive preferences

3. **Familiarity Score**
   - Favorites get bonus
   - Recently visited slight penalty (variety)
   - High group rating bonus

4. **Capacity Score**
   - Penalize if too small for group
   - Slight penalty if way too large (impersonal)

**Final Score:** Weighted combination, weights configurable per group

### Venue Display

For each venue option shown:
- Name and category
- Distance (from user's location or centroid)
- Why it matched (badges: "favorite", "chill vibe", "close for everyone")
- Reason for lower rank if applicable ("far for Sarah")

---

## 7. Matching Algorithm

### Overview

The matching algorithm runs when the Gathering phase ends. Its job:
1. Find the time window where at least 2 people overlap
2. Score venues that work for that group
3. Present ranked options for voting

### Time Window Definition

The advertised hangout time is defined as:
- **Start time:** Earliest moment when at least 2 people can be present
- **End time:** Latest moment when at least 2 people will remain

This maximizes the hangout window. People come and go; what matters is that it's never a solo affair.

### Time Overlap Calculation

```
Input: Set of responses with [earliest, latest] windows

Algorithm:
1. Create timeline of all arrivals and departures
2. Walk timeline, tracking who's available at each point
3. Find continuous window where count >= 2
4. Start = first moment with 2+ people
5. End = last moment before dropping below 2
```

**Example:**
```
Sarah:  [4:00pm -------- 7:00pm]
Mike:        [5:00pm -------- 8:00pm]
Jen:         [5:00pm ---- 6:30pm]
You:    [4:30pm -------------- 8:00pm]

Overlap Analysis:
- 4:00-4:30: Sarah only (1) ✗
- 4:30-5:00: Sarah, You (2) ✓ ← START
- 5:00-6:30: Sarah, Mike, Jen, You (4) ✓
- 6:30-7:00: Sarah, Mike, You (3) ✓
- 7:00-8:00: Mike, You (2) ✓ ← END

Advertised time: 4:30pm - 8:00pm
Peak attendance: 5:00pm - 6:30pm (all 4)
```

The timeline view shows who's there when, so attendees know the vibe at different times.

### Minimum Viable Hangout

A hangout requires at least 2 people with overlapping availability. If only 1 person responds "yes" or no two windows overlap:
- State → No Match
- Notify group: "Not enough overlap for a hangout"

### Partial Attendance Display

Since not everyone is there the whole time, the UI shows:
- Full timeline of who's arriving/leaving when
- Peak window (when most people overlap)
- Who you'll see if you arrive at time X

### Venue Scoring

For each candidate venue, calculate:

```
score = (
    w_location * location_score(venue, attendees) +
    w_preference * preference_score(venue, attendees) +
    w_familiarity * familiarity_score(venue, group) +
    w_capacity * capacity_score(venue, group_size)
)
```

**Default Weights:**
- w_location: 0.35
- w_preference: 0.30
- w_familiarity: 0.25
- w_capacity: 0.10

### Venue Filtering (Before Scoring)

Eliminate venues that:
- Exceed any attendee's max_distance
- Fall in any attendee's excluded_area
- Don't match required category (if ping specified)
- Are closed during overlap window

### Result Presentation

Return top N venues (default: 5) with:
- Venue details
- Score breakdown (shown as "why this venue")
- Who it works for / potential issues

---

## 8. Notifications

### Notification Types

#### Ping Received
- **Trigger:** New ping created in a group you're in
- **Content:** "[Name] pinged [Group]: [Activity] [Timing]?"
- **Action:** Opens ping response screen
- **Priority:** High

#### Response Updates
- **Trigger:** Someone responds to a ping you're involved in
- **Content:** "[Name] is in!" or "[Name] can't make it"
- **Action:** Opens ping detail view
- **Priority:** Medium
- **Batching:** If multiple responses in short window, combine: "3 people responded"

#### Match Ready
- **Trigger:** Gathering phase ends, matches found
- **Content:** "Time to vote! 3 venues match for [Group]"
- **Action:** Opens venue voting screen
- **Priority:** High

#### Venue Confirmed
- **Trigger:** Voting complete, venue selected
- **Content:** "It's happening! [Venue] at [Time]"
- **Action:** Opens hangout detail screen
- **Priority:** High

#### Attendee Status (During Active Hangout)
- **Trigger:** Someone's status changes
- **Content:** "[Name] is enroute" or "[Name] arrived"
- **Action:** Opens active hangout view
- **Priority:** Low
- **Batching:** Combine rapid updates

#### Cancellation
- **Trigger:** Someone who said yes cancels
- **Content:** "[Name] can no longer make it"
- **Action:** Opens ping/hangout view
- **Priority:** Medium

#### Ping Expired / No Match
- **Trigger:** Gathering ended with no viable match
- **Content:** "No match for [Activity] - not enough overlap"
- **Action:** Opens summary
- **Priority:** Low

### Notification Settings (Per User)

- **All notifications:** On/Off
- **Per-group mute:** Temporarily silence a group
- **Quiet hours:** No notifications during specified times
- **Priority only:** Only High priority notifications

---

## 9. Privacy & Security

### Core Principles

1. **End-to-End Encryption** - All user data encrypted on device before transmission
2. **Group-Only Access** - Only group members can decrypt shared data
3. **Minimal Collection** - Server stores only encrypted blobs, can't read content
4. **No Contact Mining** - Groups formed only via explicit invite codes
5. **User Control** - Users can delete their data, leave groups, revoke permissions

### Encryption Model

#### Key Distribution
- Each group has a symmetric encryption key
- Key generated when group is created
- Key distributed to members via secure key exchange
- Key rotated when members leave (forward secrecy)

#### What's Encrypted
- User profiles (name, avatar, preferences)
- Ping content
- Response content
- Venue notes and favorites
- Location data
- Chat messages (if implemented)

#### What's NOT Encrypted (Server Readable)
- User ID to group ID mappings (needed for routing)
- Timestamps (for ordering, expiration)
- Notification tokens (for push delivery)
- Encrypted blob sizes

### Location Data

#### Opt-In Model
- Location sharing disabled by default
- User explicitly enables per-group or globally
- Can revoke at any time

#### Home Location
- Stored encrypted on device and server
- Used only to determine "hasn't left yet" status
- Group members see status, not actual location

#### Live Location (During Hangout)
- Enabled only during active hangout
- Used for ETA calculation
- Group sees "enroute, X mins away" not coordinates
- Automatically disabled when hangout ends

#### Location Precision
- ETA shown as range ("5-10 mins") not exact
- "Arrived" triggered within reasonable radius (~50m)
- Server receives encrypted location, can't read it

### Data Retention

| Data Type | Retention | Notes |
|-----------|-----------|-------|
| Active ping data | Until complete/cancelled | Needed for functionality |
| Completed hangouts | 30 days | Then auto-deleted |
| User profiles | Until account deletion | Encrypted |
| Group favorites | Indefinite | Until group deleted |
| Location traces | Real-time only | Never stored |
| Encrypted blobs | Per above rules | Server can't read |

### Account Deletion

When user requests deletion:
1. Remove from all groups (triggers key rotation)
2. Delete all user data from server
3. Delete local data
4. Cannot be undone

### Invite Codes

- 6-8 alphanumeric characters
- Single-use or multi-use (group setting)
- Expiration (optional, e.g., 24 hours)
- Any group member can generate
- Can be revoked

---

## 10. Edge Cases & Error Handling

### No One Responds

**Scenario:** Ping sent, gathering timeout reached, zero responses.

**Handling:**
- State → No Match
- Notification to initiator: "No responses to your ping"
- Ping archived
- Initiator can create new ping

### Everyone Cancels

**Scenario:** Had responses, then all cancelled.

**Handling:**
- If in Gathering: Continue waiting (more might respond)
- If in Voting: State → Cancelled, notify all
- If in Active Hangout: State → Complete (empty hangout)

### No Venue Match

**Scenario:** Responses collected but no venue satisfies all constraints.

**Handling:**
- Show best partial matches with warnings
- "These venues are close but: The Bar is too far for Mike"
- Allow group to override constraints
- Fallback: "Pick any place" manual entry

### No Time Overlap

**Scenario:** No two respondents have overlapping availability (can't form a 2-person minimum).

**Handling:**
- State → No Match
- Show visualization of non-overlapping windows
- Notify group: "Can't find a time - no two schedules overlap"
- Suggest: "Closest miss: Sarah leaves at 5pm, Mike arrives at 5pm"
- Option for initiator to try again with different timing

### Network Failures

#### During Response Submission
- Queue response locally
- Retry with exponential backoff
- Show "pending" state to user
- Submit when connection restored

#### During Voting
- Vote stored locally
- Sync when reconnected
- Show last-known vote state with stale indicator

#### During Active Hangout
- Location updates pause
- Status shows "updating..."
- Resume automatically on reconnect
- Manual override buttons always work

### Conflicting Pings

**Scenario:** User is invited to two pings with overlapping times.

**Handling:**
- Allow responses to both (user manages own time)
- If both reach Venue Confirmed with overlap:
  - Warn user: "You're confirmed for 2 hangouts at 5pm"
  - Prompt to cancel one
- System doesn't auto-cancel (user's choice)

### Late Response After Venue Confirmed

**Scenario:** Someone responds "yes" after venue already selected.

**Handling:**
- Show them the confirmed venue
- Ask: "The group is going to [Venue] at [Time]. Join?"
- If yes: Add to attendee list, notify group
- If venue at capacity: Queue as "requested to join"

### Initiator Goes Offline

**Scenario:** Ping creator loses connection during critical phase.

**Handling:**
- Gathering: Continues normally (timeout-based)
- Voting: Continues normally (democratic)
- Venue selection tie: Falls to secondary tiebreakers (first responder, random)
- Initiator regains control on reconnect

### Malformed NLU Input

**Scenario:** User input can't be parsed.

**Handling:**
- First attempt: Ask for clarification naturally
  - "I didn't catch that - when are you free?"
- Second attempt: Offer structured input
  - "Want to use the time picker instead?"
- Log for training data (anonymized)

### Venue Closed / Unavailable

**Scenario:** Selected venue is actually closed at hangout time.

**Handling:**
- If detected during voting: Remove from options, note why
- If detected after confirmation:
  - Notify group: "[Venue] appears to be closed"
  - Offer re-vote with remaining options
  - Fallback to next-highest-voted venue

### Duplicate Pings

**Scenario:** User accidentally creates two similar pings.

**Handling:**
- Warn on creation: "You have an active ping for drinks. Create another?"
- Allow (might be intentional for different subgroups)
- Group members see both, can respond to each

### Group With One Member

**Scenario:** User creates group, no one else joins.

**Handling:**
- Allow creation (they'll invite later)
- Pinging a solo group warns: "You're the only one here. Invite friends?"
- Still functional (self-ping for location tracking?)

### Time Zone Handling

**Scenario:** Group members in different time zones.

**Handling:**
- All times stored in UTC
- Display in user's local timezone
- When showing group availability, note timezone differences
- "Sarah (NYC): 5pm-7pm" displayed as "Sarah: 5pm-7pm EST (2pm-4pm your time)"

---

## Appendix A: Terminology Quick Reference

| Term | Definition |
|------|------------|
| **Ping** | An invitation to gauge interest in a hangout |
| **Response** | A user's answer to a ping (yes/no + availability) |
| **Gathering** | Phase where responses are being collected |
| **Matching** | Phase where algorithm finds time/venue overlap |
| **Hangout** | A confirmed, happening event |
| **Venue** | A place where the hangout occurs |
| **Favorite** | A venue saved by the group |
| **Overlap** | Time window when multiple people are available |
| **Initiator** | Person who created the ping |
| **Attendee** | Person confirmed to attend a hangout |

---

## Appendix B: State Transition Reference

| Current State | Event | Next State | Side Effects |
|---------------|-------|------------|--------------|
| Idle | User creates ping | Ping Sent | Notify group |
| Ping Sent | First response received | Gathering | - |
| Ping Sent | Initiator cancels | Cancelled | Notify group |
| Gathering | Response received | Gathering | Update pool |
| Gathering | Timeout/threshold | Matching | Run algorithm |
| Gathering | All respond "no" | No Match | Notify initiator |
| Gathering | Initiator cancels | Cancelled | Notify group |
| Matching | Matches found | Venue Vote | Show options |
| Matching | No matches | No Match | Notify group |
| Venue Vote | Voting complete | Venue Confirmed | Notify group |
| Venue Vote | All cancel | Cancelled | Notify group |
| Venue Confirmed | Start time reached | Active Hangout | Enable location |
| Venue Confirmed | All cancel | Cancelled | Notify group |
| Active Hangout | All left/timeout | Complete | Prompt for save |
| Active Hangout | Initiator ends | Complete | Prompt for save |

---

## Appendix C: Natural Language Examples

### Ping Creation

| Input | Activity | Timing | Vibe |
|-------|----------|--------|------|
| "Who's up for drinks?" | drinks | today (implicit) | - |
| "Dinner tonight?" | dinner | tonight | - |
| "Let's grab coffee this afternoon" | coffee | this afternoon | - |
| "Anyone want to do something chill later?" | activity | later today | chill |
| "Beers after work, nothing too crazy" | drinks | after work | casual |
| "Brunch Sunday? Somewhere with outdoor seating" | brunch | Sunday | outdoor |

### Response Parsing

| Input | Answer | Earliest | Latest | Constraints |
|-------|--------|----------|--------|-------------|
| "I'm in" | yes | (ask) | (ask) | - |
| "Yes, free after 5" | yes | 5:00pm | (ask) | - |
| "Count me in, can stay til 8" | yes | (ask) | 8:00pm | - |
| "Sounds good, 5 to 7 works" | yes | 5:00pm | 7:00pm | - |
| "In but not downtown" | yes | (ask) | (ask) | exclude: downtown |
| "Maybe later, after 6" | yes | 6:00pm | (ask) | - |
| "Can't today" | no | - | - | - |
| "Pass" | no | - | - | - |

### Status Updates

| Input | Intent | Action |
|-------|--------|--------|
| "Leaving now" | update_eta | Set status: enroute |
| "Be there in 10" | update_eta | Set ETA: +10 mins |
| "Running late, 15 mins" | update_eta | Set status: delayed, ETA: +15 |
| "I'm here" | arrived | Set status: arrived |
| "Just got here" | arrived | Set status: arrived |
| "Gotta run" | leaving | Set status: left |
| "Had to leave early" | leaving | Set status: left |

---

*End of Specification*

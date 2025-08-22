# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Essential Commands

### Build
```bash
cargo build           # Build the project
cargo build --release # Build with optimizations
```

### Test
```bash
cargo test                    # Run all tests
cargo test test_name          # Run specific test by name
cargo test -- --nocapture     # Show println! output during tests
cargo test --lib              # Run only library tests
```

### Lint and Format
```bash
cargo fmt              # Format code using rustfmt
cargo fmt -- --check   # Check formatting without modifying
cargo clippy           # Run clippy linter
cargo clippy -- -D warnings  # Treat warnings as errors
```

### Check
```bash
cargo check            # Quick compilation check without producing binaries
```

## Project Architecture

This is a Rust library implementing a typestate pattern for modeling social planning and intent management. The codebase is organized around the "5 W's" concept:

### Core Modules

- **`what.rs`**: Defines the types of activities (`Activity` enum) and moods (`Mood` enum) for social planning
- **`when.rs`**: Simple availability modeling with a single `Availability` struct (start/end times). Empty Vec<Availability> = flexible
- **`where.rs`**: Currently empty, intended for location modeling
- **`who.rs`**: Currently empty, intended for participant modeling
- **`lib.rs`**: Main library entry point implementing the typestate pattern for intent management

### Key Design Pattern: Typestate

The codebase uses Rust's type system to enforce state transitions at compile time. The `Intent<S>` struct is parameterized by state types that implement `IntentState`:

1. **`Wanting`**: Initial state for an intent
2. **`Pinging`**: State after initiating contact with activity and mood
3. **`Scheduling`**: State for coordinating availability
4. **`Voting`**: State for decision making

Each state has specific methods that transition to the next valid state, preventing invalid state transitions at compile time.

### State Transition Flow
```
Wanting -> Pinging -> Scheduling -> Voting
```

The `Intent` struct maintains a history of states in its `plan` field while tracking the current state.

## Dependencies

- **chrono**: Used for date/time handling and availability windows

## Testing Approach

Tests are located in `#[cfg(test)]` modules within each source file. The test suite covers:
- State transitions in the typestate pattern
- Availability window creation and validation
- Overlapping availability detection
- Flexible availability (empty Vec) handling
- Activity and mood enums

Run a single test with: `cargo test test_intent_state_transitions`
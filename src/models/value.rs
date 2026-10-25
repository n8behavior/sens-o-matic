//! The values of the domain that can only hold a valid value.
//!
//! A rule of the API, such as "a name has 1 to 100 characters", is kept by a
//! type, not by a call that a handler must remember. Each type here has a
//! constructor that returns a `Result`, and its [`Deserialize`] goes through
//! that constructor (`#[serde(try_from = ...)]`). A request body that holds a
//! bad value then fails when [`AppJson`](super::AppJson) reads it, with `400
//! Bad Request`, and no handler validates anything. A value that exists is
//! valid, so the create and the update of an entity take the same types and
//! keep the same rules.
//!
//! | Type | Holds | Rule |
//! |---|---|---|
//! | [`BoundedText`] | text | a number of characters between two bounds |
//! | [`InviteCode`] | the code that joins a group | 6 to 8 letters or digits |
//! | [`Distance`] | miles | finite, not negative |
//! | [`Latitude`], [`Longitude`] | degrees | finite, within -90 to 90 and -180 to 180 |
//! | [`TimeWindow`] | two instants | the end is after the start |
//!
//! [`Availability`](super::Availability), [`Timeline`](super::Timeline) and
//! [`TimeOverlap`](super::TimeOverlap) are windows with their own field names
//! on the wire; each one builds on a [`TimeWindow`].
//!
//! # A refusal
//!
//! Each type has its own error. The message of an error says which kind of
//! value was refused, the range that is allowed, and what was found, and it
//! does not repeat the value, which may be long. The client gets it in the
//! `message` of the [`ApiError`](super::ApiError), after the path of the field
//! that axum found it in:
//!
//! ```text
//! Failed to deserialize the JSON body into the target type: name: text must have 1 to 100 characters, but has 0 at line 2 column 12
//! ```
//!
//! The wording is not stable; a client switches on `error`, not on `message`.
//!
//! # In the OpenAPI document
//!
//! Each type implements [`PartialSchema`], which says its bounds (`minLength`,
//! `maximum`, ...). A field that holds one repeats them in its `#[schema(...)]`
//! attribute, next to `value_type = String` (or `f64`), the same way the ids
//! do (see [`id`](super::id#in-the-openapi-document)); `inline` would give the
//! same bounds from the type, but wraps each field in a `oneOf`. The test
//! `bounds_are_in_the_schema` in [`crate::router`] compares every such field
//! with the schema of its type, so that the document cannot say something else
//! than the code does.
//!
//! # Why one generic text type
//!
//! The texts of the API have three different bounds. [`BoundedText`] has the code
//! once, with the bounds as const parameters, and the aliases [`Name`],
//! [`ActivityType`], [`RoughTiming`] and [`Vibe`] give a field the name of what
//! it holds. Six newtypes would repeat the same impls six times. The price is
//! that two aliases with the same bounds are the same type: a [`RoughTiming`]
//! can stand where an [`ActivityType`] is expected. The field names and the
//! doc comments keep them apart, and a mix-up is not an invalid value, as a
//! [`GroupId`](super::GroupId) in place of a [`UserId`](super::UserId) is.

use std::{fmt, str::FromStr};

use chrono::{DateTime, Utc};
use rand::Rng;
use serde::{Deserialize, Serialize};
use utoipa::{
    PartialSchema,
    openapi::{KnownFormat, ObjectBuilder, RefOr, Schema, SchemaFormat, Type},
};

/// The name of a user or a group: 1 to 100 characters.
pub type Name = BoundedText<1, 100>;
/// What a ping proposes to do: 1 to 50 characters.
pub type ActivityType = BoundedText<1, 50>;
/// When a ping proposes to meet, in words: 1 to 50 characters.
pub type RoughTiming = BoundedText<1, 50>;
/// The mood of a ping: up to 100 characters.
pub type Vibe = BoundedText<0, 100>;

/// Text with `MIN` to `MAX` characters.
///
/// It counts characters (Unicode scalar values), not bytes: `"é"` is one
/// character and two bytes. A bound that is not a length of text, `MIN > MAX`,
/// does not compile.
///
/// See the [module](self) for why this is one type with aliases.
///
/// # Examples
///
/// ```
/// use sens_o_matic::models::value::{BoundedText, TextError};
///
/// let name: BoundedText<1, 5> = "Ada".parse().unwrap();
/// assert_eq!(name.as_str(), "Ada");
///
/// // Too short, and too long. The error says the range and the length found.
/// let empty = "".parse::<BoundedText<1, 5>>().unwrap_err();
/// assert_eq!(empty, TextError { min: 1, max: 5, actual: 0 });
/// assert_eq!(empty.to_string(), "text must have 1 to 5 characters, but has 0");
/// assert!("Ada Lovelace".parse::<BoundedText<1, 5>>().is_err());
///
/// // Characters count, not bytes.
/// assert!("ééééé".parse::<BoundedText<1, 5>>().is_ok());
///
/// // A body is refused where it is read.
/// assert!(serde_json::from_str::<BoundedText<1, 5>>(r#""Ada""#).is_ok());
/// assert!(serde_json::from_str::<BoundedText<1, 5>>(r#""""#).is_err());
/// ```
///
/// The bounds must make sense. This does not compile:
///
/// ```compile_fail,E0080
/// use sens_o_matic::models::value::BoundedText;
///
/// let _ = "x".parse::<BoundedText<5, 1>>();
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash, Deserialize)]
#[serde(try_from = "String")]
pub struct BoundedText<const MIN: usize, const MAX: usize>(String);

/// The text has too few or too many characters. See [`BoundedText`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("text must have {min} to {max} characters, but has {actual}")]
pub struct TextError {
    /// The fewest characters that were allowed.
    pub min: usize,
    /// The most characters that were allowed.
    pub max: usize,
    /// The number of characters that the text had.
    pub actual: usize,
}

impl<const MIN: usize, const MAX: usize> BoundedText<MIN, MAX> {
    /// Fails the build of any `BoundedText<MIN, MAX>` with `MIN > MAX`, which
    /// no text can satisfy.
    const BOUNDS_MAKE_SENSE: () = assert!(
        MIN <= MAX,
        "BoundedText needs MIN <= MAX, or no text is valid. See the module docs of models::value"
    );

    /// The text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl<const MIN: usize, const MAX: usize> TryFrom<String> for BoundedText<MIN, MAX> {
    type Error = TextError;

    fn try_from(text: String) -> Result<Self, Self::Error> {
        let () = Self::BOUNDS_MAKE_SENSE;
        let actual = text.chars().count();
        if (MIN..=MAX).contains(&actual) {
            Ok(Self(text))
        } else {
            Err(TextError {
                min: MIN,
                max: MAX,
                actual,
            })
        }
    }
}

impl<const MIN: usize, const MAX: usize> FromStr for BoundedText<MIN, MAX> {
    type Err = TextError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        text.to_string().try_into()
    }
}

impl<const MIN: usize, const MAX: usize> From<BoundedText<MIN, MAX>> for String {
    fn from(text: BoundedText<MIN, MAX>) -> Self {
        text.0
    }
}

impl<const MIN: usize, const MAX: usize> AsRef<str> for BoundedText<MIN, MAX> {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl<const MIN: usize, const MAX: usize> fmt::Display for BoundedText<MIN, MAX> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl<const MIN: usize, const MAX: usize> Serialize for BoundedText<MIN, MAX> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.serialize(serializer)
    }
}

/// A string with the bounds of the type (`minLength` is left out when it is 0).
impl<const MIN: usize, const MAX: usize> PartialSchema for BoundedText<MIN, MAX> {
    fn schema() -> RefOr<Schema> {
        ObjectBuilder::new()
            .schema_type(Type::String)
            .min_length((MIN > 0).then_some(MIN))
            .max_length(Some(MAX))
            .into()
    }
}

/// The code that lets a person join a group: 6 to 8 letters or digits, and the
/// case matters.
///
/// The server makes codes of 8 characters ([`InviteCode::random`]); a join
/// request may carry 6 to 8, a wider range than the server produces. The code
/// is not checked for uniqueness among the groups: with 62⁸ possible codes a
/// collision is unlikely, and if one happened, joining would reach whichever of
/// the two groups the store finds first.
///
/// # Examples
///
/// ```
/// use sens_o_matic::models::value::{InviteCode, InviteCodeError};
///
/// let code: InviteCode = "abc123".parse().unwrap();
/// assert_eq!(code.as_str(), "abc123");
/// assert_eq!(InviteCode::random().as_str().len(), 8);
///
/// // 5 characters: too short. A dash is not a letter or a digit.
/// assert_eq!("abc12".parse::<InviteCode>(), Err(InviteCodeError::Length { actual: 5 }));
/// assert_eq!("abc-1234".parse::<InviteCode>(), Err(InviteCodeError::Character { found: '-' }));
/// assert_eq!(
///     "abc-1234".parse::<InviteCode>().unwrap_err().to_string(),
///     "invite code must be letters and digits, but has '-'"
/// );
///
/// // A body is refused where it is read.
/// assert!(serde_json::from_str::<InviteCode>(r#""abc123""#).is_ok());
/// assert!(serde_json::from_str::<InviteCode>(r#""invalid!""#).is_err());
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash, Deserialize)]
#[serde(try_from = "String")]
pub struct InviteCode(String);

/// Why a string is not an [`InviteCode`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum InviteCodeError {
    /// The string does not have 6 to 8 characters.
    #[error(
        "invite code must have {} to {} characters, but has {actual}",
        InviteCode::MIN_LEN,
        InviteCode::MAX_LEN
    )]
    Length {
        /// The number of characters that the string had.
        actual: usize,
    },
    /// The string has a character that is not an ASCII letter or digit.
    #[error("invite code must be letters and digits, but has {found:?}")]
    Character {
        /// The first character that is not a letter or a digit.
        found: char,
    },
}

impl InviteCode {
    /// The fewest characters of a code.
    const MIN_LEN: usize = 6;
    /// The most characters of a code, and the length of the codes that the
    /// server makes.
    const MAX_LEN: usize = 8;

    /// Makes a random code of 8 characters, each an ASCII letter (either case)
    /// or a digit.
    pub fn random() -> Self {
        const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
        let mut rng = rand::rng();
        let code = (0..Self::MAX_LEN)
            .map(|_| char::from(CHARSET[rng.random_range(0..CHARSET.len())]))
            .collect();
        Self(code)
    }

    /// The code.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for InviteCode {
    type Error = InviteCodeError;

    fn try_from(code: String) -> Result<Self, Self::Error> {
        let actual = code.chars().count();
        if !(Self::MIN_LEN..=Self::MAX_LEN).contains(&actual) {
            return Err(InviteCodeError::Length { actual });
        }
        match code.chars().find(|c| !c.is_ascii_alphanumeric()) {
            Some(found) => Err(InviteCodeError::Character { found }),
            None => Ok(Self(code)),
        }
    }
}

impl FromStr for InviteCode {
    type Err = InviteCodeError;

    fn from_str(code: &str) -> Result<Self, Self::Err> {
        code.to_string().try_into()
    }
}

impl AsRef<str> for InviteCode {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for InviteCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl Serialize for InviteCode {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.serialize(serializer)
    }
}

/// A string of 6 to 8 characters (`minLength`, `maxLength`) that are letters or
/// digits (`pattern`).
impl PartialSchema for InviteCode {
    fn schema() -> RefOr<Schema> {
        ObjectBuilder::new()
            .schema_type(Type::String)
            .min_length(Some(Self::MIN_LEN))
            .max_length(Some(Self::MAX_LEN))
            .pattern(Some("^[A-Za-z0-9]+$"))
            .into()
    }
}

/// How far, in miles: a finite number that is not negative.
///
/// # Examples
///
/// ```
/// use sens_o_matic::models::value::{Distance, DistanceError};
///
/// let walk = Distance::try_from(0.5).unwrap();
/// assert_eq!(walk.miles(), 0.5);
/// assert!(Distance::try_from(0.0).is_ok());
///
/// // Negative, and not a number.
/// assert_eq!(Distance::try_from(-5.0), Err(DistanceError(-5.0)));
/// assert_eq!(
///     Distance::try_from(-5.0).unwrap_err().to_string(),
///     "distance must be a finite number of miles, 0 or more, but is -5"
/// );
/// assert!(Distance::try_from(f64::NAN).is_err());
/// assert!(Distance::try_from(f64::INFINITY).is_err());
///
/// // A body is refused where it is read.
/// assert!(serde_json::from_str::<Distance>("0.5").is_ok());
/// assert!(serde_json::from_str::<Distance>("-5.0").is_err());
/// ```
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Deserialize)]
#[serde(try_from = "f64")]
pub struct Distance(f64);

/// The number is not a [`Distance`]. It holds the number.
#[derive(Debug, Clone, Copy, PartialEq, thiserror::Error)]
#[error("distance must be a finite number of miles, 0 or more, but is {0}")]
pub struct DistanceError(pub f64);

impl Distance {
    /// The distance in miles.
    pub fn miles(self) -> f64 {
        self.0
    }
}

impl TryFrom<f64> for Distance {
    type Error = DistanceError;

    fn try_from(miles: f64) -> Result<Self, Self::Error> {
        if miles.is_finite() && miles >= 0.0 {
            Ok(Self(miles))
        } else {
            Err(DistanceError(miles))
        }
    }
}

impl From<Distance> for f64 {
    fn from(distance: Distance) -> Self {
        distance.0
    }
}

impl Serialize for Distance {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.serialize(serializer)
    }
}

/// A number (`double`) that is at least 0 (`minimum`).
impl PartialSchema for Distance {
    fn schema() -> RefOr<Schema> {
        double_schema(0.0, None)
    }
}

/// How far north or south of the equator, in degrees: a number from -90 (the
/// south pole) to 90 (the north pole).
///
/// # Examples
///
/// ```
/// use sens_o_matic::models::value::{Latitude, LatitudeError};
///
/// assert_eq!(Latitude::try_from(40.7).unwrap().degrees(), 40.7);
/// assert!(Latitude::try_from(-90.0).is_ok());
///
/// assert_eq!(Latitude::try_from(91.0), Err(LatitudeError(91.0)));
/// assert_eq!(
///     Latitude::try_from(91.0).unwrap_err().to_string(),
///     "latitude must be a number of degrees from -90 to 90, but is 91"
/// );
/// assert!(Latitude::try_from(f64::NAN).is_err());
///
/// // A body is refused where it is read.
/// assert!(serde_json::from_str::<Latitude>("40.7").is_ok());
/// assert!(serde_json::from_str::<Latitude>("100").is_err());
/// ```
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Deserialize)]
#[serde(try_from = "f64")]
pub struct Latitude(f64);

/// The number is not a [`Latitude`]. It holds the number.
#[derive(Debug, Clone, Copy, PartialEq, thiserror::Error)]
#[error("latitude must be a number of degrees from -90 to 90, but is {0}")]
pub struct LatitudeError(pub f64);

impl Latitude {
    /// The latitude in degrees; negative is south.
    pub fn degrees(self) -> f64 {
        self.0
    }
}

impl TryFrom<f64> for Latitude {
    type Error = LatitudeError;

    fn try_from(degrees: f64) -> Result<Self, Self::Error> {
        if (-90.0..=90.0).contains(&degrees) {
            Ok(Self(degrees))
        } else {
            Err(LatitudeError(degrees))
        }
    }
}

impl From<Latitude> for f64 {
    fn from(latitude: Latitude) -> Self {
        latitude.0
    }
}

impl Serialize for Latitude {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.serialize(serializer)
    }
}

/// A number (`double`) from -90 to 90 (`minimum`, `maximum`).
impl PartialSchema for Latitude {
    fn schema() -> RefOr<Schema> {
        double_schema(-90.0, Some(90.0))
    }
}

/// How far east or west of the prime meridian, in degrees: a number from -180
/// to 180.
///
/// # Examples
///
/// ```
/// use sens_o_matic::models::value::{Longitude, LongitudeError};
///
/// assert_eq!(Longitude::try_from(-74.0).unwrap().degrees(), -74.0);
/// assert!(Longitude::try_from(180.0).is_ok());
///
/// assert_eq!(Longitude::try_from(-181.0), Err(LongitudeError(-181.0)));
/// assert_eq!(
///     Longitude::try_from(-181.0).unwrap_err().to_string(),
///     "longitude must be a number of degrees from -180 to 180, but is -181"
/// );
/// assert!(Longitude::try_from(f64::INFINITY).is_err());
///
/// // A body is refused where it is read.
/// assert!(serde_json::from_str::<Longitude>("-74.0").is_ok());
/// assert!(serde_json::from_str::<Longitude>("200").is_err());
/// ```
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Deserialize)]
#[serde(try_from = "f64")]
pub struct Longitude(f64);

/// The number is not a [`Longitude`]. It holds the number.
#[derive(Debug, Clone, Copy, PartialEq, thiserror::Error)]
#[error("longitude must be a number of degrees from -180 to 180, but is {0}")]
pub struct LongitudeError(pub f64);

impl Longitude {
    /// The longitude in degrees; negative is west.
    pub fn degrees(self) -> f64 {
        self.0
    }
}

impl TryFrom<f64> for Longitude {
    type Error = LongitudeError;

    fn try_from(degrees: f64) -> Result<Self, Self::Error> {
        if (-180.0..=180.0).contains(&degrees) {
            Ok(Self(degrees))
        } else {
            Err(LongitudeError(degrees))
        }
    }
}

impl From<Longitude> for f64 {
    fn from(longitude: Longitude) -> Self {
        longitude.0
    }
}

impl Serialize for Longitude {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.serialize(serializer)
    }
}

/// A number (`double`) from -180 to 180 (`minimum`, `maximum`).
impl PartialSchema for Longitude {
    fn schema() -> RefOr<Schema> {
        double_schema(-180.0, Some(180.0))
    }
}

/// The schema of a number of the type `double`, with a lower bound and
/// possibly an upper one.
fn double_schema(minimum: f64, maximum: Option<f64>) -> RefOr<Schema> {
    ObjectBuilder::new()
        .schema_type(Type::Number)
        .format(Some(SchemaFormat::KnownFormat(KnownFormat::Double)))
        .minimum(Some(minimum))
        .maximum(maximum)
        .into()
}

/// A span of time that has a length: its end is strictly after its start.
///
/// It is the rule that [`Availability`](super::Availability),
/// [`Timeline`](super::Timeline) and [`TimeOverlap`](super::TimeOverlap) have
/// in common. An empty window, where the end is the start, is refused as well:
/// two members who are free until 18:00 and from 18:00 on have no time in
/// common.
///
/// # Examples
///
/// ```
/// use chrono::{TimeZone, Utc};
/// use sens_o_matic::models::value::TimeWindow;
///
/// let at = |hour| Utc.with_ymd_and_hms(2024, 12, 15, hour, 0, 0).unwrap();
///
/// let window = TimeWindow::new(at(17), at(21)).unwrap();
/// assert_eq!((window.start(), window.end()), (at(17), at(21)));
///
/// // The end before the start, and a window of no length.
/// assert!(TimeWindow::new(at(21), at(17)).is_err());
/// assert!(TimeWindow::new(at(17), at(17)).is_err());
/// assert_eq!(
///     TimeWindow::new(at(21), at(17)).unwrap_err().to_string(),
///     "time window must end after it starts, but starts at 2024-12-15T21:00:00+00:00 \
///      and ends at 2024-12-15T17:00:00+00:00"
/// );
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimeWindow {
    start: DateTime<Utc>,
    end: DateTime<Utc>,
}

/// The end of the span is not after its start. See [`TimeWindow`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error(
    "time window must end after it starts, but starts at {} and ends at {}",
    .start.to_rfc3339(),
    .end.to_rfc3339()
)]
pub struct TimeWindowError {
    /// The start that was given.
    pub start: DateTime<Utc>,
    /// The end that was given.
    pub end: DateTime<Utc>,
}

impl TimeWindow {
    /// Makes a window from its start and its end.
    ///
    /// # Errors
    ///
    /// Returns a [`TimeWindowError`] when `end` is the same as `start` or
    /// before it.
    pub fn new(start: DateTime<Utc>, end: DateTime<Utc>) -> Result<Self, TimeWindowError> {
        if end > start {
            Ok(Self { start, end })
        } else {
            Err(TimeWindowError { start, end })
        }
    }

    /// When the window starts.
    pub fn start(&self) -> DateTime<Utc> {
        self.start
    }

    /// When the window ends; always after [`TimeWindow::start`].
    pub fn end(&self) -> DateTime<Utc> {
        self.end
    }
}

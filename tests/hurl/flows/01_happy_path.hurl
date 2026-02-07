# Happy Path Flow Test
# Complete journey from user creation to hangout completion

# ============================================================================
# Step 1: Create Users
# ============================================================================

POST {{BASE_URL}}/api/users
Content-Type: application/json
{
  "name": "Sarah",
  "preferences": {
    "default_distance": 5.0,
    "favorite_areas": ["downtown", "midtown"]
  }
}
HTTP 201
[Captures]
sarah_id: jsonpath "$.id"


POST {{BASE_URL}}/api/users
Content-Type: application/json
{
  "name": "Mike",
  "preferences": {
    "default_distance": 3.0,
    "favorite_areas": ["downtown"]
  }
}
HTTP 201
[Captures]
mike_id: jsonpath "$.id"


POST {{BASE_URL}}/api/users
Content-Type: application/json
{
  "name": "Jen",
  "preferences": {
    "default_distance": 4.0
  }
}
HTTP 201
[Captures]
jen_id: jsonpath "$.id"


# ============================================================================
# Step 2: Create Group and Add Members
# ============================================================================

POST {{BASE_URL}}/api/groups
Content-Type: application/json
{
  "name": "Friday Night Crew",
  "creator_id": "{{sarah_id}}"
}
HTTP 201
[Captures]
group_id: jsonpath "$.id"
invite_code: jsonpath "$.invite_code"
[Asserts]
jsonpath "$.members" count == 1


POST {{BASE_URL}}/api/groups/join
Content-Type: application/json
{
  "user_id": "{{mike_id}}",
  "invite_code": "{{invite_code}}"
}
HTTP 200
[Asserts]
jsonpath "$.members" count == 2


POST {{BASE_URL}}/api/groups/join
Content-Type: application/json
{
  "user_id": "{{jen_id}}",
  "invite_code": "{{invite_code}}"
}
HTTP 200
[Asserts]
jsonpath "$.members" count == 3


# ============================================================================
# Step 3: Create Ping
# ============================================================================

POST {{BASE_URL}}/api/pings
Content-Type: application/json
{
  "initiator": "{{sarah_id}}",
  "group": "{{group_id}}",
  "activity_type": "drinks",
  "rough_timing": "tonight",
  "vibe": "chill"
}
HTTP 201
[Captures]
ping_id: jsonpath "$.id"
[Asserts]
jsonpath "$.state" == "ping_sent"


# ============================================================================
# Step 4: Gather Responses
# ============================================================================

# Mike responds yes
POST {{BASE_URL}}/api/pings/{{ping_id}}/responses
Content-Type: application/json
{
  "user": "{{mike_id}}",
  "answer": true,
  "availability": {
    "earliest": "2024-12-15T17:00:00Z",
    "latest": "2024-12-15T21:00:00Z"
  },
  "preferences": {
    "max_distance": 3.0,
    "preferred_areas": ["downtown"]
  }
}
HTTP 201
[Captures]
mike_response_id: jsonpath "$.id"


# Verify state transition to gathering
GET {{BASE_URL}}/api/pings/{{ping_id}}
HTTP 200
[Asserts]
jsonpath "$.state" == "gathering"


# Jen responds yes
POST {{BASE_URL}}/api/pings/{{ping_id}}/responses
Content-Type: application/json
{
  "user": "{{jen_id}}",
  "answer": true,
  "availability": {
    "earliest": "2024-12-15T18:00:00Z",
    "latest": "2024-12-15T22:00:00Z"
  }
}
HTTP 201


# Verify we have two responses
GET {{BASE_URL}}/api/pings/{{ping_id}}
HTTP 200
[Asserts]
jsonpath "$.state" == "gathering"
jsonpath "$.responses" count == 2


# ============================================================================
# Step 5: Trigger Matching
# ============================================================================

POST {{BASE_URL}}/api/pings/{{ping_id}}/match
Content-Type: application/json
{
  "user_id": "{{sarah_id}}"
}
HTTP 200
[Asserts]
jsonpath "$.state" == "matching"


# Check match results
GET {{BASE_URL}}/api/pings/{{ping_id}}/match-results
HTTP 200
[Asserts]
jsonpath "$.has_match" == true
jsonpath "$.overlap.attendee_count" == 2


# ============================================================================
# Step 6: Confirm Hangout
# ============================================================================

POST {{BASE_URL}}/api/pings/{{ping_id}}/confirm
Content-Type: application/json
{
  "user_id": "{{sarah_id}}",
  "timeline": {
    "start": "2024-12-15T18:00:00Z",
    "end": "2024-12-15T21:00:00Z"
  }
}
HTTP 201
[Asserts]
jsonpath "$.state" == "venue_confirmed"
jsonpath "$.hangout" exists


# Verify ping state
GET {{BASE_URL}}/api/pings/{{ping_id}}
HTTP 200
[Asserts]
jsonpath "$.state" == "venue_confirmed"
jsonpath "$.hangout.timeline.start" exists


# ============================================================================
# Step 7: Activate Hangout
# ============================================================================

POST {{BASE_URL}}/api/pings/{{ping_id}}/activate
HTTP 200
[Asserts]
jsonpath "$.state" == "active_hangout"


GET {{BASE_URL}}/api/pings/{{ping_id}}
HTTP 200
[Asserts]
jsonpath "$.state" == "active_hangout"


# ============================================================================
# Step 8: Update Attendee Statuses
# ============================================================================

# Mike is enroute
PUT {{BASE_URL}}/api/pings/{{ping_id}}/attendees/{{mike_id}}/status
Content-Type: application/json
{
  "status": "enroute"
}
HTTP 200
[Asserts]
jsonpath "$.hangout.attendee_statuses['{{mike_id}}']" == "enroute"


# Mike arrives
PUT {{BASE_URL}}/api/pings/{{ping_id}}/attendees/{{mike_id}}/status
Content-Type: application/json
{
  "status": "arrived"
}
HTTP 200
[Asserts]
jsonpath "$.hangout.attendee_statuses['{{mike_id}}']" == "arrived"


# Jen arrives
PUT {{BASE_URL}}/api/pings/{{ping_id}}/attendees/{{jen_id}}/status
Content-Type: application/json
{
  "status": "arrived"
}
HTTP 200


# Mike leaves
PUT {{BASE_URL}}/api/pings/{{ping_id}}/attendees/{{mike_id}}/status
Content-Type: application/json
{
  "status": "left"
}
HTTP 200


# ============================================================================
# Step 9: Complete Hangout
# ============================================================================

POST {{BASE_URL}}/api/pings/{{ping_id}}/complete
HTTP 200
[Asserts]
jsonpath "$.state" == "complete"


# Verify final states
GET {{BASE_URL}}/api/pings/{{ping_id}}
HTTP 200
[Asserts]
jsonpath "$.state" == "complete"
jsonpath "$.hangout" exists

# Hangout CRUD Tests

# Setup: Create full flow to get to hangout
POST {{BASE_URL}}/api/users
Content-Type: application/json
{
  "name": "Hangout Host"
}
HTTP 201
[Captures]
host_id: jsonpath "$.id"


POST {{BASE_URL}}/api/users
Content-Type: application/json
{
  "name": "Attendee One"
}
HTTP 201
[Captures]
attendee1_id: jsonpath "$.id"


POST {{BASE_URL}}/api/users
Content-Type: application/json
{
  "name": "Attendee Two"
}
HTTP 201
[Captures]
attendee2_id: jsonpath "$.id"


POST {{BASE_URL}}/api/groups
Content-Type: application/json
{
  "name": "Hangout Test Group",
  "creator_id": "{{host_id}}"
}
HTTP 201
[Captures]
group_id: jsonpath "$.id"
invite_code: jsonpath "$.invite_code"


POST {{BASE_URL}}/api/groups/join
Content-Type: application/json
{
  "user_id": "{{attendee1_id}}",
  "invite_code": "{{invite_code}}"
}
HTTP 200


POST {{BASE_URL}}/api/groups/join
Content-Type: application/json
{
  "user_id": "{{attendee2_id}}",
  "invite_code": "{{invite_code}}"
}
HTTP 200


# Create ping
POST {{BASE_URL}}/api/pings
Content-Type: application/json
{
  "initiator": "{{host_id}}",
  "group": "{{group_id}}",
  "activity_type": "drinks",
  "rough_timing": "tonight"
}
HTTP 201
[Captures]
ping_id: jsonpath "$.id"


# Add responses
POST {{BASE_URL}}/api/pings/{{ping_id}}/responses
Content-Type: application/json
{
  "user": "{{attendee1_id}}",
  "answer": true,
  "availability": {
    "earliest": "2024-12-15T18:00:00Z",
    "latest": "2024-12-15T22:00:00Z"
  }
}
HTTP 201


POST {{BASE_URL}}/api/pings/{{ping_id}}/responses
Content-Type: application/json
{
  "user": "{{attendee2_id}}",
  "answer": true,
  "availability": {
    "earliest": "2024-12-15T17:00:00Z",
    "latest": "2024-12-15T21:00:00Z"
  }
}
HTTP 201


# Trigger matching
POST {{BASE_URL}}/api/pings/{{ping_id}}/match
Content-Type: application/json
{
  "user_id": "{{host_id}}"
}
HTTP 200
[Asserts]
jsonpath "$.state" == "matching"


# Get match results
GET {{BASE_URL}}/api/pings/{{ping_id}}/match-results
HTTP 200
[Asserts]
jsonpath "$.ping_id" == {{ping_id}}
jsonpath "$.has_match" == true
jsonpath "$.overlap.start" exists
jsonpath "$.overlap.end" exists
jsonpath "$.overlap.attendee_count" >= 2


# Confirm hangout
POST {{BASE_URL}}/api/pings/{{ping_id}}/confirm
Content-Type: application/json
{
  "user_id": "{{host_id}}",
  "timeline": {
    "start": "2024-12-15T18:00:00Z",
    "end": "2024-12-15T21:00:00Z"
  }
}
HTTP 201
[Captures]
hangout_id: jsonpath "$.id"
[Asserts]
jsonpath "$.ping" == {{ping_id}}
jsonpath "$.status" == "confirmed"
jsonpath "$.confirmed_attendees" count >= 1
jsonpath "$.timeline.start" exists
jsonpath "$.attendee_statuses" exists


# Get hangout by ID
GET {{BASE_URL}}/api/hangouts/{{hangout_id}}
HTTP 200
[Asserts]
jsonpath "$.id" == {{hangout_id}}
jsonpath "$.status" == "confirmed"


# Activate hangout
POST {{BASE_URL}}/api/hangouts/{{hangout_id}}/activate
HTTP 200
[Asserts]
jsonpath "$.status" == "active"


# Update attendee status to enroute
PUT {{BASE_URL}}/api/hangouts/{{hangout_id}}/attendees/{{attendee1_id}}/status
Content-Type: application/json
{
  "status": "enroute"
}
HTTP 200
[Asserts]
jsonpath "$.attendee_statuses['{{attendee1_id}}']" == "enroute"


# Update attendee status to arrived
PUT {{BASE_URL}}/api/hangouts/{{hangout_id}}/attendees/{{attendee1_id}}/status
Content-Type: application/json
{
  "status": "arrived"
}
HTTP 200
[Asserts]
jsonpath "$.attendee_statuses['{{attendee1_id}}']" == "arrived"


# Complete hangout
POST {{BASE_URL}}/api/hangouts/{{hangout_id}}/complete
HTTP 200
[Asserts]
jsonpath "$.status" == "complete"


# Verify ping state updated
GET {{BASE_URL}}/api/pings/{{ping_id}}
HTTP 200
[Asserts]
jsonpath "$.state" == "complete"

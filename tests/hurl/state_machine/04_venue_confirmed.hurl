# State Machine: Venue Confirmed State Tests

# Setup - full flow to venue_confirmed
POST {{BASE_URL}}/api/users
Content-Type: application/json
{
  "name": "Initiator"
}
HTTP 201
[Captures]
initiator_id: jsonpath "$.id"


POST {{BASE_URL}}/api/users
Content-Type: application/json
{
  "name": "Attendee"
}
HTTP 201
[Captures]
attendee_id: jsonpath "$.id"


POST {{BASE_URL}}/api/groups
Content-Type: application/json
{
  "name": "Venue Confirmed Test Group",
  "creator_id": "{{initiator_id}}"
}
HTTP 201
[Captures]
group_id: jsonpath "$.id"
invite_code: jsonpath "$.invite_code"


POST {{BASE_URL}}/api/groups/join
Content-Type: application/json
{
  "user_id": "{{attendee_id}}",
  "invite_code": "{{invite_code}}"
}
HTTP 200


POST {{BASE_URL}}/api/pings
Content-Type: application/json
{
  "initiator": "{{initiator_id}}",
  "group": "{{group_id}}",
  "activity_type": "coffee",
  "rough_timing": "this afternoon"
}
HTTP 201
[Captures]
ping_id: jsonpath "$.id"


POST {{BASE_URL}}/api/pings/{{ping_id}}/responses
Content-Type: application/json
{
  "user": "{{attendee_id}}",
  "answer": true,
  "availability": {
    "earliest": "2024-12-15T14:00:00Z",
    "latest": "2024-12-15T17:00:00Z"
  }
}
HTTP 201


POST {{BASE_URL}}/api/pings/{{ping_id}}/match
Content-Type: application/json
{
  "user_id": "{{initiator_id}}"
}
HTTP 200


POST {{BASE_URL}}/api/pings/{{ping_id}}/confirm
Content-Type: application/json
{
  "user_id": "{{initiator_id}}",
  "timeline": {
    "start": "2024-12-15T14:00:00Z",
    "end": "2024-12-15T17:00:00Z"
  }
}
HTTP 201
[Captures]
hangout_id: jsonpath "$.id"


# Verify venue_confirmed state
GET {{BASE_URL}}/api/pings/{{ping_id}}
HTTP 200
[Asserts]
jsonpath "$.state" == "venue_confirmed"


# Hangout is in confirmed status
GET {{BASE_URL}}/api/hangouts/{{hangout_id}}
HTTP 200
[Asserts]
jsonpath "$.status" == "confirmed"
jsonpath "$.timeline.start" exists
jsonpath "$.timeline.end" exists


# Cannot trigger matching again
POST {{BASE_URL}}/api/pings/{{ping_id}}/match
Content-Type: application/json
{
  "user_id": "{{initiator_id}}"
}
HTTP 409


# Cannot confirm again
POST {{BASE_URL}}/api/pings/{{ping_id}}/confirm
Content-Type: application/json
{
  "user_id": "{{initiator_id}}",
  "timeline": {
    "start": "2024-12-15T14:00:00Z",
    "end": "2024-12-15T17:00:00Z"
  }
}
HTTP 409


# Can activate the hangout
POST {{BASE_URL}}/api/hangouts/{{hangout_id}}/activate
HTTP 200
[Asserts]
jsonpath "$.status" == "active"


# Ping state updated to active_hangout
GET {{BASE_URL}}/api/pings/{{ping_id}}
HTTP 200
[Asserts]
jsonpath "$.state" == "active_hangout"

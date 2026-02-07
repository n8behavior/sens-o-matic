# State Machine: Active Hangout State Tests

# Setup - full flow to active hangout
POST {{BASE_URL}}/api/users
Content-Type: application/json
{
  "name": "Host"
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
  "name": "Active Hangout Test Group",
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


POST {{BASE_URL}}/api/pings/{{ping_id}}/match
Content-Type: application/json
{
  "user_id": "{{host_id}}"
}
HTTP 200


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


# Activate ping
POST {{BASE_URL}}/api/pings/{{ping_id}}/activate
HTTP 200
[Asserts]
jsonpath "$.state" == "active_hangout"


# Verify active_hangout state
GET {{BASE_URL}}/api/pings/{{ping_id}}
HTTP 200
[Asserts]
jsonpath "$.state" == "active_hangout"


# Attendees can update their status
PUT {{BASE_URL}}/api/pings/{{ping_id}}/attendees/{{attendee1_id}}/status
Content-Type: application/json
{
  "status": "enroute"
}
HTTP 200
[Asserts]
jsonpath "$.hangout.attendee_statuses['{{attendee1_id}}']" == "enroute"


PUT {{BASE_URL}}/api/pings/{{ping_id}}/attendees/{{attendee1_id}}/status
Content-Type: application/json
{
  "status": "arrived"
}
HTTP 200
[Asserts]
jsonpath "$.hangout.attendee_statuses['{{attendee1_id}}']" == "arrived"


PUT {{BASE_URL}}/api/pings/{{ping_id}}/attendees/{{attendee2_id}}/status
Content-Type: application/json
{
  "status": "arrived"
}
HTTP 200


# Update to left status
PUT {{BASE_URL}}/api/pings/{{ping_id}}/attendees/{{attendee1_id}}/status
Content-Type: application/json
{
  "status": "left"
}
HTTP 200
[Asserts]
jsonpath "$.hangout.attendee_statuses['{{attendee1_id}}']" == "left"


# Cannot activate again
POST {{BASE_URL}}/api/pings/{{ping_id}}/activate
HTTP 409


# Can complete ping
POST {{BASE_URL}}/api/pings/{{ping_id}}/complete
HTTP 200
[Asserts]
jsonpath "$.state" == "complete"


# Ping state updated to complete
GET {{BASE_URL}}/api/pings/{{ping_id}}
HTTP 200
[Asserts]
jsonpath "$.state" == "complete"

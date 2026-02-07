# State Machine: Complete State Tests

# Setup - full flow to complete
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
  "name": "Attendee"
}
HTTP 201
[Captures]
attendee_id: jsonpath "$.id"


POST {{BASE_URL}}/api/groups
Content-Type: application/json
{
  "name": "Complete Test Group",
  "creator_id": "{{host_id}}"
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
  "initiator": "{{host_id}}",
  "group": "{{group_id}}",
  "activity_type": "dinner",
  "rough_timing": "tonight"
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
    "earliest": "2024-12-15T19:00:00Z",
    "latest": "2024-12-15T22:00:00Z"
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
    "start": "2024-12-15T19:00:00Z",
    "end": "2024-12-15T22:00:00Z"
  }
}
HTTP 201
[Captures]
hangout_id: jsonpath "$.id"


POST {{BASE_URL}}/api/hangouts/{{hangout_id}}/activate
HTTP 200


POST {{BASE_URL}}/api/hangouts/{{hangout_id}}/complete
HTTP 200


# Verify complete state
GET {{BASE_URL}}/api/pings/{{ping_id}}
HTTP 200
[Asserts]
jsonpath "$.state" == "complete"


GET {{BASE_URL}}/api/hangouts/{{hangout_id}}
HTTP 200
[Asserts]
jsonpath "$.status" == "complete"


# Complete is terminal - cannot transition further

# Cannot cancel
POST {{BASE_URL}}/api/pings/{{ping_id}}/cancel
Content-Type: application/json
{
  "user_id": "{{host_id}}"
}
HTTP 409
[Asserts]
jsonpath "$.error" exists


# Cannot add responses
POST {{BASE_URL}}/api/pings/{{ping_id}}/responses
Content-Type: application/json
{
  "user": "{{host_id}}",
  "answer": true,
  "availability": {
    "earliest": "2024-12-15T19:00:00Z",
    "latest": "2024-12-15T22:00:00Z"
  }
}
HTTP 409


# Cannot trigger matching
POST {{BASE_URL}}/api/pings/{{ping_id}}/match
Content-Type: application/json
{
  "user_id": "{{host_id}}"
}
HTTP 409


# Cannot activate hangout
POST {{BASE_URL}}/api/hangouts/{{hangout_id}}/activate
HTTP 409


# Cannot complete again
POST {{BASE_URL}}/api/hangouts/{{hangout_id}}/complete
HTTP 409


# Can still read data
GET {{BASE_URL}}/api/pings/{{ping_id}}
HTTP 200
[Asserts]
jsonpath "$.state" == "complete"


GET {{BASE_URL}}/api/hangouts/{{hangout_id}}
HTTP 200
[Asserts]
jsonpath "$.status" == "complete"

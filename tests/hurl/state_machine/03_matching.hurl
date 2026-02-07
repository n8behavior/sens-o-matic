# State Machine: Matching State Tests

# Setup
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
  "name": "Member One"
}
HTTP 201
[Captures]
member1_id: jsonpath "$.id"


POST {{BASE_URL}}/api/users
Content-Type: application/json
{
  "name": "Member Two"
}
HTTP 201
[Captures]
member2_id: jsonpath "$.id"


POST {{BASE_URL}}/api/groups
Content-Type: application/json
{
  "name": "Matching Test Group",
  "creator_id": "{{initiator_id}}"
}
HTTP 201
[Captures]
group_id: jsonpath "$.id"
invite_code: jsonpath "$.invite_code"


POST {{BASE_URL}}/api/groups/join
Content-Type: application/json
{
  "user_id": "{{member1_id}}",
  "invite_code": "{{invite_code}}"
}
HTTP 200


POST {{BASE_URL}}/api/groups/join
Content-Type: application/json
{
  "user_id": "{{member2_id}}",
  "invite_code": "{{invite_code}}"
}
HTTP 200


# Create ping and add responses
POST {{BASE_URL}}/api/pings
Content-Type: application/json
{
  "initiator": "{{initiator_id}}",
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
  "user": "{{member1_id}}",
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
  "user": "{{member2_id}}",
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
  "user_id": "{{initiator_id}}"
}
HTTP 200
[Asserts]
jsonpath "$.state" == "matching"


# Match results are available
GET {{BASE_URL}}/api/pings/{{ping_id}}/match-results
HTTP 200
[Asserts]
jsonpath "$.ping_id" == {{ping_id}}
jsonpath "$.has_match" == true
jsonpath "$.overlap" exists
jsonpath "$.overlap.start" exists
jsonpath "$.overlap.end" exists
jsonpath "$.overlap.attendee_count" == 2


# Cannot trigger matching again
POST {{BASE_URL}}/api/pings/{{ping_id}}/match
Content-Type: application/json
{
  "user_id": "{{initiator_id}}"
}
HTTP 409
[Asserts]
jsonpath "$.error" exists


# Cannot add new responses in matching state
POST {{BASE_URL}}/api/pings/{{ping_id}}/responses
Content-Type: application/json
{
  "user": "{{initiator_id}}",
  "answer": true,
  "availability": {
    "earliest": "2024-12-15T18:00:00Z",
    "latest": "2024-12-15T22:00:00Z"
  }
}
HTTP 409
[Asserts]
jsonpath "$.error" exists


# Can confirm hangout from matching
POST {{BASE_URL}}/api/pings/{{ping_id}}/confirm
Content-Type: application/json
{
  "user_id": "{{initiator_id}}",
  "timeline": {
    "start": "2024-12-15T18:00:00Z",
    "end": "2024-12-15T21:00:00Z"
  }
}
HTTP 201
[Captures]
hangout_id: jsonpath "$.id"
[Asserts]
jsonpath "$.status" == "confirmed"


# Ping state updated to venue_confirmed
GET {{BASE_URL}}/api/pings/{{ping_id}}
HTTP 200
[Asserts]
jsonpath "$.state" == "venue_confirmed"
jsonpath "$.hangout_id" == {{hangout_id}}

# State Machine: No Match State Tests

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
  "name": "No Match Test Group",
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


# Test: Only negative responses results in no_match
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


# All responses are negative
POST {{BASE_URL}}/api/pings/{{ping_id}}/responses
Content-Type: application/json
{
  "user": "{{member1_id}}",
  "answer": false
}
HTTP 201


POST {{BASE_URL}}/api/pings/{{ping_id}}/responses
Content-Type: application/json
{
  "user": "{{member2_id}}",
  "answer": false
}
HTTP 201


# Trigger matching with all negative responses
POST {{BASE_URL}}/api/pings/{{ping_id}}/match
Content-Type: application/json
{
  "user_id": "{{initiator_id}}"
}
HTTP 200
[Asserts]
jsonpath "$.state" == "no_match"


# Verify no_match state
GET {{BASE_URL}}/api/pings/{{ping_id}}
HTTP 200
[Asserts]
jsonpath "$.state" == "no_match"


# Match results show no match
GET {{BASE_URL}}/api/pings/{{ping_id}}/match-results
HTTP 200
[Asserts]
jsonpath "$.has_match" == false
jsonpath "$.overlap" == null


# No_match is terminal - cannot transition further

# Cannot add responses
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


# Cannot trigger matching
POST {{BASE_URL}}/api/pings/{{ping_id}}/match
Content-Type: application/json
{
  "user_id": "{{initiator_id}}"
}
HTTP 409


# Cannot confirm hangout
POST {{BASE_URL}}/api/pings/{{ping_id}}/confirm
Content-Type: application/json
{
  "user_id": "{{initiator_id}}",
  "timeline": {
    "start": "2024-12-15T18:00:00Z",
    "end": "2024-12-15T21:00:00Z"
  }
}
HTTP 409


# Cannot cancel (already terminal)
POST {{BASE_URL}}/api/pings/{{ping_id}}/cancel
Content-Type: application/json
{
  "user_id": "{{initiator_id}}"
}
HTTP 409


# Can still read ping
GET {{BASE_URL}}/api/pings/{{ping_id}}
HTTP 200
[Asserts]
jsonpath "$.state" == "no_match"

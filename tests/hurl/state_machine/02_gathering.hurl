# State Machine: Gathering State Tests

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
  "name": "Gathering Test Group",
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


# Create ping
POST {{BASE_URL}}/api/pings
Content-Type: application/json
{
  "initiator": "{{initiator_id}}",
  "group": "{{group_id}}",
  "activity_type": "dinner",
  "rough_timing": "tonight"
}
HTTP 201
[Captures]
ping_id: jsonpath "$.id"


# First response - transitions to gathering
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
[Captures]
response1_id: jsonpath "$.id"


GET {{BASE_URL}}/api/pings/{{ping_id}}
HTTP 200
[Asserts]
jsonpath "$.state" == "gathering"


# Additional responses can be added
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


# Still in gathering state
GET {{BASE_URL}}/api/pings/{{ping_id}}
HTTP 200
[Asserts]
jsonpath "$.state" == "gathering"
jsonpath "$.responses" count == 2


# Responses can be modified
PUT {{BASE_URL}}/api/pings/{{ping_id}}/responses/{{response1_id}}
Content-Type: application/json
{
  "user": "{{member1_id}}",
  "availability": {
    "earliest": "2024-12-15T19:00:00Z",
    "latest": "2024-12-15T23:00:00Z"
  }
}
HTTP 200
[Asserts]
jsonpath "$.availability.earliest" == "2024-12-15T19:00:00Z"


# Cannot confirm hangout from gathering
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
[Asserts]
jsonpath "$.error" exists


# Initiator can trigger matching
POST {{BASE_URL}}/api/pings/{{ping_id}}/match
Content-Type: application/json
{
  "user_id": "{{initiator_id}}"
}
HTTP 200
[Asserts]
jsonpath "$.state" == "matching"


# Verify transition to matching
GET {{BASE_URL}}/api/pings/{{ping_id}}
HTTP 200
[Asserts]
jsonpath "$.state" == "matching"

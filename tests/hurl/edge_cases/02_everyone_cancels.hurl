# Edge Case: Everyone Cancels
# Tests handling when all positive responses change to negative

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
  "name": "Flaky One"
}
HTTP 201
[Captures]
flaky1_id: jsonpath "$.id"


POST {{BASE_URL}}/api/users
Content-Type: application/json
{
  "name": "Flaky Two"
}
HTTP 201
[Captures]
flaky2_id: jsonpath "$.id"


POST {{BASE_URL}}/api/groups
Content-Type: application/json
{
  "name": "Flaky Group",
  "creator_id": "{{initiator_id}}"
}
HTTP 201
[Captures]
group_id: jsonpath "$.id"
invite_code: jsonpath "$.invite_code"


POST {{BASE_URL}}/api/groups/join
Content-Type: application/json
{
  "user_id": "{{flaky1_id}}",
  "invite_code": "{{invite_code}}"
}
HTTP 200


POST {{BASE_URL}}/api/groups/join
Content-Type: application/json
{
  "user_id": "{{flaky2_id}}",
  "invite_code": "{{invite_code}}"
}
HTTP 200


# Create ping
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


# Both members respond yes
POST {{BASE_URL}}/api/pings/{{ping_id}}/responses
Content-Type: application/json
{
  "user": "{{flaky1_id}}",
  "answer": true,
  "availability": {
    "earliest": "2024-12-15T18:00:00Z",
    "latest": "2024-12-15T22:00:00Z"
  }
}
HTTP 201
[Captures]
response1_id: jsonpath "$.id"


POST {{BASE_URL}}/api/pings/{{ping_id}}/responses
Content-Type: application/json
{
  "user": "{{flaky2_id}}",
  "answer": true,
  "availability": {
    "earliest": "2024-12-15T17:00:00Z",
    "latest": "2024-12-15T21:00:00Z"
  }
}
HTTP 201
[Captures]
response2_id: jsonpath "$.id"


# Verify gathering state with positive responses
GET {{BASE_URL}}/api/pings/{{ping_id}}
HTTP 200
[Asserts]
jsonpath "$.state" == "gathering"


# First person cancels
PUT {{BASE_URL}}/api/pings/{{ping_id}}/responses/{{response1_id}}
Content-Type: application/json
{
  "user": "{{flaky1_id}}",
  "answer": false
}
HTTP 200
[Asserts]
jsonpath "$.answer" == false


# Still in gathering (one person still yes)
GET {{BASE_URL}}/api/pings/{{ping_id}}
HTTP 200
[Asserts]
jsonpath "$.state" == "gathering"


# Second person cancels
PUT {{BASE_URL}}/api/pings/{{ping_id}}/responses/{{response2_id}}
Content-Type: application/json
{
  "user": "{{flaky2_id}}",
  "answer": false
}
HTTP 200
[Asserts]
jsonpath "$.answer" == false


# Still in gathering (waiting for more responses)
GET {{BASE_URL}}/api/pings/{{ping_id}}
HTTP 200
[Asserts]
jsonpath "$.state" == "gathering"


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

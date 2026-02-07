# State Machine: Cancelled State Tests

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
  "name": "Member"
}
HTTP 201
[Captures]
member_id: jsonpath "$.id"


POST {{BASE_URL}}/api/groups
Content-Type: application/json
{
  "name": "Cancel Test Group",
  "creator_id": "{{initiator_id}}"
}
HTTP 201
[Captures]
group_id: jsonpath "$.id"
invite_code: jsonpath "$.invite_code"


POST {{BASE_URL}}/api/groups/join
Content-Type: application/json
{
  "user_id": "{{member_id}}",
  "invite_code": "{{invite_code}}"
}
HTTP 200


# Test 1: Cancel from ping_sent state
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
ping1_id: jsonpath "$.id"


POST {{BASE_URL}}/api/pings/{{ping1_id}}/cancel
Content-Type: application/json
{
  "user_id": "{{initiator_id}}"
}
HTTP 200
[Asserts]
jsonpath "$.state" == "cancelled"


# Verify cancelled state
GET {{BASE_URL}}/api/pings/{{ping1_id}}
HTTP 200
[Asserts]
jsonpath "$.state" == "cancelled"


# Test 2: Cancel from gathering state
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
ping2_id: jsonpath "$.id"


POST {{BASE_URL}}/api/pings/{{ping2_id}}/responses
Content-Type: application/json
{
  "user": "{{member_id}}",
  "answer": true,
  "availability": {
    "earliest": "2024-12-15T14:00:00Z",
    "latest": "2024-12-15T17:00:00Z"
  }
}
HTTP 201


GET {{BASE_URL}}/api/pings/{{ping2_id}}
HTTP 200
[Asserts]
jsonpath "$.state" == "gathering"


POST {{BASE_URL}}/api/pings/{{ping2_id}}/cancel
Content-Type: application/json
{
  "user_id": "{{initiator_id}}"
}
HTTP 200
[Asserts]
jsonpath "$.state" == "cancelled"


# Cancelled is terminal - cannot transition further

# Cannot add responses
POST {{BASE_URL}}/api/pings/{{ping1_id}}/responses
Content-Type: application/json
{
  "user": "{{member_id}}",
  "answer": true,
  "availability": {
    "earliest": "2024-12-15T18:00:00Z",
    "latest": "2024-12-15T22:00:00Z"
  }
}
HTTP 409


# Cannot trigger matching
POST {{BASE_URL}}/api/pings/{{ping1_id}}/match
Content-Type: application/json
{
  "user_id": "{{initiator_id}}"
}
HTTP 409


# Cannot cancel again
POST {{BASE_URL}}/api/pings/{{ping1_id}}/cancel
Content-Type: application/json
{
  "user_id": "{{initiator_id}}"
}
HTTP 409


# Only initiator can cancel
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
ping3_id: jsonpath "$.id"


POST {{BASE_URL}}/api/pings/{{ping3_id}}/cancel
Content-Type: application/json
{
  "user_id": "{{member_id}}"
}
HTTP 403
[Asserts]
jsonpath "$.error" exists


# Can still read cancelled ping
GET {{BASE_URL}}/api/pings/{{ping1_id}}
HTTP 200
[Asserts]
jsonpath "$.state" == "cancelled"

# Response CRUD Tests

# Setup: Create users, group, and ping
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
  "name": "Responder One"
}
HTTP 201
[Captures]
responder1_id: jsonpath "$.id"


POST {{BASE_URL}}/api/users
Content-Type: application/json
{
  "name": "Responder Two"
}
HTTP 201
[Captures]
responder2_id: jsonpath "$.id"


POST {{BASE_URL}}/api/groups
Content-Type: application/json
{
  "name": "Response Test Group",
  "creator_id": "{{initiator_id}}"
}
HTTP 201
[Captures]
group_id: jsonpath "$.id"
invite_code: jsonpath "$.invite_code"


# Add members to group
POST {{BASE_URL}}/api/groups/join
Content-Type: application/json
{
  "user_id": "{{responder1_id}}",
  "invite_code": "{{invite_code}}"
}
HTTP 200


POST {{BASE_URL}}/api/groups/join
Content-Type: application/json
{
  "user_id": "{{responder2_id}}",
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


# Submit positive response with full details
POST {{BASE_URL}}/api/pings/{{ping_id}}/responses
Content-Type: application/json
{
  "user": "{{responder1_id}}",
  "answer": true,
  "availability": {
    "earliest": "2024-12-15T17:00:00Z",
    "latest": "2024-12-15T21:00:00Z"
  },
  "preferences": {
    "max_distance": 3.0,
    "preferred_areas": ["downtown"],
    "excluded_areas": ["suburbs"]
  }
}
HTTP 201
[Captures]
response1_id: jsonpath "$.id"
[Asserts]
jsonpath "$.user" == {{responder1_id}}
jsonpath "$.answer" == true
jsonpath "$.availability.earliest" exists
jsonpath "$.availability.latest" exists
jsonpath "$.preferences.max_distance" == 3.0
jsonpath "$.updated_at" exists


# Verify ping state changed to gathering
GET {{BASE_URL}}/api/pings/{{ping_id}}
HTTP 200
[Asserts]
jsonpath "$.state" == "gathering"
jsonpath "$.responses" count == 1


# Submit negative response (minimal data)
POST {{BASE_URL}}/api/pings/{{ping_id}}/responses
Content-Type: application/json
{
  "user": "{{responder2_id}}",
  "answer": false
}
HTTP 201
[Captures]
response2_id: jsonpath "$.id"
[Asserts]
jsonpath "$.user" == {{responder2_id}}
jsonpath "$.answer" == false
jsonpath "$.availability" == null


# Verify ping has both responses
GET {{BASE_URL}}/api/pings/{{ping_id}}
HTTP 200
[Asserts]
jsonpath "$.responses" count == 2


# Update response: change from no to yes
PUT {{BASE_URL}}/api/pings/{{ping_id}}/responses/{{response2_id}}
Content-Type: application/json
{
  "user": "{{responder2_id}}",
  "answer": true,
  "availability": {
    "earliest": "2024-12-15T18:00:00Z",
    "latest": "2024-12-15T20:00:00Z"
  }
}
HTTP 200
[Asserts]
jsonpath "$.answer" == true
jsonpath "$.availability.earliest" exists


# Update response: change availability
PUT {{BASE_URL}}/api/pings/{{ping_id}}/responses/{{response1_id}}
Content-Type: application/json
{
  "user": "{{responder1_id}}",
  "availability": {
    "earliest": "2024-12-15T17:30:00Z",
    "latest": "2024-12-15T22:00:00Z"
  }
}
HTTP 200
[Asserts]
jsonpath "$.availability.earliest" == "2024-12-15T17:30:00Z"

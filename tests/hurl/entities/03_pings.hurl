# Ping CRUD Tests

# Setup: Create users and group
POST {{BASE_URL}}/api/users
Content-Type: application/json
{
  "name": "Ping Creator"
}
HTTP 201
[Captures]
initiator_id: jsonpath "$.id"


POST {{BASE_URL}}/api/users
Content-Type: application/json
{
  "name": "Group Member"
}
HTTP 201
[Captures]
member_id: jsonpath "$.id"


POST {{BASE_URL}}/api/groups
Content-Type: application/json
{
  "name": "Test Group",
  "creator_id": "{{initiator_id}}"
}
HTTP 201
[Captures]
group_id: jsonpath "$.id"
invite_code: jsonpath "$.invite_code"


# Add member to group
POST {{BASE_URL}}/api/groups/join
Content-Type: application/json
{
  "user_id": "{{member_id}}",
  "invite_code": "{{invite_code}}"
}
HTTP 200


# Create a ping
POST {{BASE_URL}}/api/pings
Content-Type: application/json
{
  "initiator": "{{initiator_id}}",
  "group": "{{group_id}}",
  "activity_type": "drinks",
  "rough_timing": "tonight",
  "vibe": "chill"
}
HTTP 201
[Captures]
ping_id: jsonpath "$.id"
[Asserts]
jsonpath "$.initiator" == {{initiator_id}}
jsonpath "$.group" == {{group_id}}
jsonpath "$.activity_type" == "drinks"
jsonpath "$.rough_timing" == "tonight"
jsonpath "$.vibe" == "chill"
jsonpath "$.state" == "ping_sent"
jsonpath "$.created_at" exists


# Get ping by ID
GET {{BASE_URL}}/api/pings/{{ping_id}}
HTTP 200
[Asserts]
jsonpath "$.id" == {{ping_id}}
jsonpath "$.state" == "ping_sent"


# Create ping without vibe (optional field)
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
[Asserts]
jsonpath "$.vibe" == null


# List pings in group
GET {{BASE_URL}}/api/groups/{{group_id}}/pings
HTTP 200
[Asserts]
jsonpath "$" count == 2


# Filter pings by state
GET {{BASE_URL}}/api/groups/{{group_id}}/pings?state=ping_sent
HTTP 200
[Asserts]
jsonpath "$" count == 2

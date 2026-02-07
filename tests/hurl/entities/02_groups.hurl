# Group CRUD Tests

# First, create a user to be the group creator
POST {{BASE_URL}}/api/users
Content-Type: application/json
{
  "name": "Group Creator"
}
HTTP 201
[Captures]
creator_id: jsonpath "$.id"


# Create a second user to join the group
POST {{BASE_URL}}/api/users
Content-Type: application/json
{
  "name": "Group Member"
}
HTTP 201
[Captures]
member_id: jsonpath "$.id"


# Create a group
POST {{BASE_URL}}/api/groups
Content-Type: application/json
{
  "name": "Friday Night Crew",
  "creator_id": "{{creator_id}}"
}
HTTP 201
[Captures]
group_id: jsonpath "$.id"
invite_code: jsonpath "$.invite_code"
[Asserts]
jsonpath "$.name" == "Friday Night Crew"
jsonpath "$.members" count == 1
jsonpath "$.members[0]" == {{creator_id}}
jsonpath "$.invite_code" matches "^[A-Za-z0-9]{6,8}$"


# Get group by ID
GET {{BASE_URL}}/api/groups/{{group_id}}
HTTP 200
[Asserts]
jsonpath "$.id" == {{group_id}}
jsonpath "$.name" == "Friday Night Crew"


# Join group via invite code
POST {{BASE_URL}}/api/groups/join
Content-Type: application/json
{
  "user_id": "{{member_id}}",
  "invite_code": "{{invite_code}}"
}
HTTP 200
[Asserts]
jsonpath "$.members" count == 2


# Verify user's groups now includes the group
GET {{BASE_URL}}/api/users/{{member_id}}/groups
HTTP 200
[Asserts]
jsonpath "$" count == 1
jsonpath "$[0].id" == {{group_id}}


# Regenerate invite code
POST {{BASE_URL}}/api/groups/{{group_id}}/regenerate-invite
Content-Type: application/json
{
  "user_id": "{{creator_id}}"
}
HTTP 200
[Captures]
new_invite_code: jsonpath "$.invite_code"
[Asserts]
jsonpath "$.invite_code" matches "^[A-Za-z0-9]{6,8}$"


# Get group pings (empty initially)
GET {{BASE_URL}}/api/groups/{{group_id}}/pings
HTTP 200
[Asserts]
jsonpath "$" count == 0


# Leave group
POST {{BASE_URL}}/api/groups/{{group_id}}/leave
Content-Type: application/json
{
  "user_id": "{{member_id}}"
}
HTTP 204


# Verify member left
GET {{BASE_URL}}/api/groups/{{group_id}}
HTTP 200
[Asserts]
jsonpath "$.members" count == 1

# Error Tests: Validation Errors

# ============================================================================
# User Validation
# ============================================================================

# Create user without name (required field)
POST {{BASE_URL}}/api/users
Content-Type: application/json
{
  "avatar": "https://example.com/avatar.jpg"
}
HTTP 400
[Asserts]
jsonpath "$.error" exists
jsonpath "$.message" exists


# Create user with empty name
POST {{BASE_URL}}/api/users
Content-Type: application/json
{
  "name": ""
}
HTTP 400
[Asserts]
jsonpath "$.error" exists


# Create user with name too long
POST {{BASE_URL}}/api/users
Content-Type: application/json
{
  "name": "This name is way too long and exceeds the maximum allowed length of one hundred characters which should trigger a validation error"
}
HTTP 400
[Asserts]
jsonpath "$.error" exists


# ============================================================================
# Group Validation
# ============================================================================

# Create valid user for group tests
POST {{BASE_URL}}/api/users
Content-Type: application/json
{
  "name": "Test User"
}
HTTP 201
[Captures]
user_id: jsonpath "$.id"


# Create group without name
POST {{BASE_URL}}/api/groups
Content-Type: application/json
{
  "creator_id": "{{user_id}}"
}
HTTP 400
[Asserts]
jsonpath "$.error" exists


# Create group without creator
POST {{BASE_URL}}/api/groups
Content-Type: application/json
{
  "name": "Test Group"
}
HTTP 400
[Asserts]
jsonpath "$.error" exists


# Join group with invalid invite code format
POST {{BASE_URL}}/api/groups/join
Content-Type: application/json
{
  "user_id": "{{user_id}}",
  "invite_code": "invalid!"
}
HTTP 400
[Asserts]
jsonpath "$.error" exists


# ============================================================================
# Ping Validation
# ============================================================================

# Create group for ping tests
POST {{BASE_URL}}/api/groups
Content-Type: application/json
{
  "name": "Validation Test Group",
  "creator_id": "{{user_id}}"
}
HTTP 201
[Captures]
group_id: jsonpath "$.id"


# Create ping without activity_type
POST {{BASE_URL}}/api/pings
Content-Type: application/json
{
  "initiator": "{{user_id}}",
  "group": "{{group_id}}",
  "rough_timing": "tonight"
}
HTTP 400
[Asserts]
jsonpath "$.error" exists


# Create ping without rough_timing
POST {{BASE_URL}}/api/pings
Content-Type: application/json
{
  "initiator": "{{user_id}}",
  "group": "{{group_id}}",
  "activity_type": "drinks"
}
HTTP 400
[Asserts]
jsonpath "$.error" exists


# Create ping without initiator
POST {{BASE_URL}}/api/pings
Content-Type: application/json
{
  "group": "{{group_id}}",
  "activity_type": "drinks",
  "rough_timing": "tonight"
}
HTTP 400
[Asserts]
jsonpath "$.error" exists


# ============================================================================
# Response Validation
# ============================================================================

# Create ping for response tests
POST {{BASE_URL}}/api/pings
Content-Type: application/json
{
  "initiator": "{{user_id}}",
  "group": "{{group_id}}",
  "activity_type": "drinks",
  "rough_timing": "tonight"
}
HTTP 201
[Captures]
ping_id: jsonpath "$.id"


# Response without user
POST {{BASE_URL}}/api/pings/{{ping_id}}/responses
Content-Type: application/json
{
  "answer": true,
  "availability": {
    "earliest": "2024-12-15T18:00:00Z",
    "latest": "2024-12-15T22:00:00Z"
  }
}
HTTP 400
[Asserts]
jsonpath "$.error" exists


# Response without answer
POST {{BASE_URL}}/api/pings/{{ping_id}}/responses
Content-Type: application/json
{
  "user": "{{user_id}}",
  "availability": {
    "earliest": "2024-12-15T18:00:00Z",
    "latest": "2024-12-15T22:00:00Z"
  }
}
HTTP 400
[Asserts]
jsonpath "$.error" exists


# Response with invalid availability (latest before earliest)
POST {{BASE_URL}}/api/pings/{{ping_id}}/responses
Content-Type: application/json
{
  "user": "{{user_id}}",
  "answer": true,
  "availability": {
    "earliest": "2024-12-15T22:00:00Z",
    "latest": "2024-12-15T18:00:00Z"
  }
}
HTTP 400
[Asserts]
jsonpath "$.error" exists


# Response with negative max_distance
POST {{BASE_URL}}/api/pings/{{ping_id}}/responses
Content-Type: application/json
{
  "user": "{{user_id}}",
  "answer": true,
  "availability": {
    "earliest": "2024-12-15T18:00:00Z",
    "latest": "2024-12-15T22:00:00Z"
  },
  "preferences": {
    "max_distance": -5.0
  }
}
HTTP 400
[Asserts]
jsonpath "$.error" exists

# Change Response Flow Test
# Tests the ability to change responses during gathering phase

# ============================================================================
# Setup
# ============================================================================

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
  "name": "Indecisive User"
}
HTTP 201
[Captures]
user_id: jsonpath "$.id"


POST {{BASE_URL}}/api/users
Content-Type: application/json
{
  "name": "Steady User"
}
HTTP 201
[Captures]
steady_id: jsonpath "$.id"


POST {{BASE_URL}}/api/groups
Content-Type: application/json
{
  "name": "Change Response Test Group",
  "creator_id": "{{host_id}}"
}
HTTP 201
[Captures]
group_id: jsonpath "$.id"
invite_code: jsonpath "$.invite_code"


POST {{BASE_URL}}/api/groups/join
Content-Type: application/json
{
  "user_id": "{{user_id}}",
  "invite_code": "{{invite_code}}"
}
HTTP 200


POST {{BASE_URL}}/api/groups/join
Content-Type: application/json
{
  "user_id": "{{steady_id}}",
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


# ============================================================================
# Initial Responses
# ============================================================================

# User initially says no
POST {{BASE_URL}}/api/pings/{{ping_id}}/responses
Content-Type: application/json
{
  "user": "{{user_id}}",
  "answer": false
}
HTTP 201
[Captures]
response_id: jsonpath "$.id"
[Asserts]
jsonpath "$.answer" == false


# Steady user says yes
POST {{BASE_URL}}/api/pings/{{ping_id}}/responses
Content-Type: application/json
{
  "user": "{{steady_id}}",
  "answer": true,
  "availability": {
    "earliest": "2024-12-15T18:00:00Z",
    "latest": "2024-12-15T22:00:00Z"
  }
}
HTTP 201


# Verify both responses
GET {{BASE_URL}}/api/pings/{{ping_id}}
HTTP 200
[Asserts]
jsonpath "$.responses" count == 2


# ============================================================================
# Change Mind: No -> Yes
# ============================================================================

# User changes mind and wants to come
PUT {{BASE_URL}}/api/pings/{{ping_id}}/responses/{{response_id}}
Content-Type: application/json
{
  "user": "{{user_id}}",
  "answer": true,
  "availability": {
    "earliest": "2024-12-15T19:00:00Z",
    "latest": "2024-12-15T23:00:00Z"
  }
}
HTTP 200
[Asserts]
jsonpath "$.answer" == true
jsonpath "$.availability.earliest" == "2024-12-15T19:00:00Z"


# ============================================================================
# Change Availability
# ============================================================================

# User's schedule changes
PUT {{BASE_URL}}/api/pings/{{ping_id}}/responses/{{response_id}}
Content-Type: application/json
{
  "user": "{{user_id}}",
  "availability": {
    "earliest": "2024-12-15T20:00:00Z",
    "latest": "2024-12-15T23:00:00Z"
  }
}
HTTP 200
[Asserts]
jsonpath "$.availability.earliest" == "2024-12-15T20:00:00Z"


# ============================================================================
# Add Preferences
# ============================================================================

# User adds preferences
PUT {{BASE_URL}}/api/pings/{{ping_id}}/responses/{{response_id}}
Content-Type: application/json
{
  "user": "{{user_id}}",
  "preferences": {
    "max_distance": 2.0,
    "preferred_areas": ["downtown"],
    "excluded_areas": ["suburbs"]
  }
}
HTTP 200
[Asserts]
jsonpath "$.preferences.max_distance" == 2.0
jsonpath "$.preferences.preferred_areas[0]" == "downtown"


# ============================================================================
# Change Mind: Yes -> No
# ============================================================================

# User can no longer make it
PUT {{BASE_URL}}/api/pings/{{ping_id}}/responses/{{response_id}}
Content-Type: application/json
{
  "user": "{{user_id}}",
  "answer": false
}
HTTP 200
[Asserts]
jsonpath "$.answer" == false


# ============================================================================
# Verify Final State
# ============================================================================

GET {{BASE_URL}}/api/pings/{{ping_id}}
HTTP 200
[Asserts]
jsonpath "$.state" == "gathering"
jsonpath "$.responses" count == 2


# ============================================================================
# Trigger Match and Proceed
# ============================================================================

# Change back to yes so we can continue
PUT {{BASE_URL}}/api/pings/{{ping_id}}/responses/{{response_id}}
Content-Type: application/json
{
  "user": "{{user_id}}",
  "answer": true,
  "availability": {
    "earliest": "2024-12-15T19:00:00Z",
    "latest": "2024-12-15T22:00:00Z"
  }
}
HTTP 200


POST {{BASE_URL}}/api/pings/{{ping_id}}/match
Content-Type: application/json
{
  "user_id": "{{host_id}}"
}
HTTP 200
[Asserts]
jsonpath "$.state" == "matching"


# Cannot change response after matching
PUT {{BASE_URL}}/api/pings/{{ping_id}}/responses/{{response_id}}
Content-Type: application/json
{
  "user": "{{user_id}}",
  "answer": false
}
HTTP 409
[Asserts]
jsonpath "$.error" exists

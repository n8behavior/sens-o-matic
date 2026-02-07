# Error Tests: Not Found Errors

# ============================================================================
# User Not Found
# ============================================================================

# Get non-existent user
GET {{BASE_URL}}/api/users/00000000-0000-0000-0000-000000000000
HTTP 404
[Asserts]
jsonpath "$.error" exists
jsonpath "$.message" exists


# Update non-existent user
PATCH {{BASE_URL}}/api/users/00000000-0000-0000-0000-000000000000
Content-Type: application/json
{
  "name": "Updated Name"
}
HTTP 404
[Asserts]
jsonpath "$.error" exists


# Delete non-existent user
DELETE {{BASE_URL}}/api/users/00000000-0000-0000-0000-000000000000
HTTP 404
[Asserts]
jsonpath "$.error" exists


# Get groups for non-existent user
GET {{BASE_URL}}/api/users/00000000-0000-0000-0000-000000000000/groups
HTTP 404
[Asserts]
jsonpath "$.error" exists


# ============================================================================
# Group Not Found
# ============================================================================

# Get non-existent group
GET {{BASE_URL}}/api/groups/00000000-0000-0000-0000-000000000000
HTTP 404
[Asserts]
jsonpath "$.error" exists


# Leave non-existent group
POST {{BASE_URL}}/api/groups/00000000-0000-0000-0000-000000000000/leave
Content-Type: application/json
{
  "user_id": "00000000-0000-0000-0000-000000000001"
}
HTTP 404
[Asserts]
jsonpath "$.error" exists


# Regenerate invite for non-existent group
POST {{BASE_URL}}/api/groups/00000000-0000-0000-0000-000000000000/regenerate-invite
Content-Type: application/json
{
  "user_id": "00000000-0000-0000-0000-000000000001"
}
HTTP 404
[Asserts]
jsonpath "$.error" exists


# Get pings for non-existent group
GET {{BASE_URL}}/api/groups/00000000-0000-0000-0000-000000000000/pings
HTTP 404
[Asserts]
jsonpath "$.error" exists


# Join with non-existent invite code
POST {{BASE_URL}}/api/groups/join
Content-Type: application/json
{
  "user_id": "00000000-0000-0000-0000-000000000001",
  "invite_code": "INVALID"
}
HTTP 404
[Asserts]
jsonpath "$.error" exists


# ============================================================================
# Ping Not Found
# ============================================================================

# Get non-existent ping
GET {{BASE_URL}}/api/pings/00000000-0000-0000-0000-000000000000
HTTP 404
[Asserts]
jsonpath "$.error" exists


# Cancel non-existent ping
POST {{BASE_URL}}/api/pings/00000000-0000-0000-0000-000000000000/cancel
Content-Type: application/json
{
  "user_id": "00000000-0000-0000-0000-000000000001"
}
HTTP 404
[Asserts]
jsonpath "$.error" exists


# Add response to non-existent ping
POST {{BASE_URL}}/api/pings/00000000-0000-0000-0000-000000000000/responses
Content-Type: application/json
{
  "user": "00000000-0000-0000-0000-000000000001",
  "answer": true
}
HTTP 404
[Asserts]
jsonpath "$.error" exists


# Trigger match for non-existent ping
POST {{BASE_URL}}/api/pings/00000000-0000-0000-0000-000000000000/match
Content-Type: application/json
{
  "user_id": "00000000-0000-0000-0000-000000000001"
}
HTTP 404
[Asserts]
jsonpath "$.error" exists


# Get match results for non-existent ping
GET {{BASE_URL}}/api/pings/00000000-0000-0000-0000-000000000000/match-results
HTTP 404
[Asserts]
jsonpath "$.error" exists


# Confirm non-existent ping
POST {{BASE_URL}}/api/pings/00000000-0000-0000-0000-000000000000/confirm
Content-Type: application/json
{
  "user_id": "00000000-0000-0000-0000-000000000001",
  "timeline": {
    "start": "2024-12-15T18:00:00Z",
    "end": "2024-12-15T21:00:00Z"
  }
}
HTTP 404
[Asserts]
jsonpath "$.error" exists


# ============================================================================
# Response Not Found
# ============================================================================

# Setup for response not found tests
POST {{BASE_URL}}/api/users
Content-Type: application/json
{
  "name": "Test User"
}
HTTP 201
[Captures]
user_id: jsonpath "$.id"


POST {{BASE_URL}}/api/groups
Content-Type: application/json
{
  "name": "Test Group",
  "creator_id": "{{user_id}}"
}
HTTP 201
[Captures]
group_id: jsonpath "$.id"


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


# Update non-existent response
PUT {{BASE_URL}}/api/pings/{{ping_id}}/responses/00000000-0000-0000-0000-000000000000
Content-Type: application/json
{
  "user": "{{user_id}}",
  "answer": true
}
HTTP 404
[Asserts]
jsonpath "$.error" exists


# ============================================================================
# Hangout Not Found
# ============================================================================

# Get non-existent hangout
GET {{BASE_URL}}/api/hangouts/00000000-0000-0000-0000-000000000000
HTTP 404
[Asserts]
jsonpath "$.error" exists


# Activate non-existent hangout
POST {{BASE_URL}}/api/hangouts/00000000-0000-0000-0000-000000000000/activate
HTTP 404
[Asserts]
jsonpath "$.error" exists


# Complete non-existent hangout
POST {{BASE_URL}}/api/hangouts/00000000-0000-0000-0000-000000000000/complete
HTTP 404
[Asserts]
jsonpath "$.error" exists


# Update attendee status for non-existent hangout
PUT {{BASE_URL}}/api/hangouts/00000000-0000-0000-0000-000000000000/attendees/00000000-0000-0000-0000-000000000001/status
Content-Type: application/json
{
  "status": "arrived"
}
HTTP 404
[Asserts]
jsonpath "$.error" exists

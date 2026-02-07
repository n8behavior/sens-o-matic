# Error Tests: Conflict Errors (State Violations)

# ============================================================================
# Setup
# ============================================================================

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
  "name": "Conflict Test Group",
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


# ============================================================================
# Duplicate Response
# ============================================================================

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


# First response from member
POST {{BASE_URL}}/api/pings/{{ping1_id}}/responses
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


# Duplicate response from same member
POST {{BASE_URL}}/api/pings/{{ping1_id}}/responses
Content-Type: application/json
{
  "user": "{{member1_id}}",
  "answer": false
}
HTTP 409
[Asserts]
jsonpath "$.error" exists
jsonpath "$.message" exists


# ============================================================================
# Invalid State Transitions
# ============================================================================

# Create a new ping for state transition tests
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
ping2_id: jsonpath "$.id"


# Cannot match from ping_sent (no responses)
POST {{BASE_URL}}/api/pings/{{ping2_id}}/match
Content-Type: application/json
{
  "user_id": "{{initiator_id}}"
}
HTTP 409
[Asserts]
jsonpath "$.error" exists


# Cannot confirm from ping_sent
POST {{BASE_URL}}/api/pings/{{ping2_id}}/confirm
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


# Add responses
POST {{BASE_URL}}/api/pings/{{ping2_id}}/responses
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


POST {{BASE_URL}}/api/pings/{{ping2_id}}/responses
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


# Cannot confirm from gathering
POST {{BASE_URL}}/api/pings/{{ping2_id}}/confirm
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


# Trigger matching
POST {{BASE_URL}}/api/pings/{{ping2_id}}/match
Content-Type: application/json
{
  "user_id": "{{initiator_id}}"
}
HTTP 200


# Cannot trigger matching again
POST {{BASE_URL}}/api/pings/{{ping2_id}}/match
Content-Type: application/json
{
  "user_id": "{{initiator_id}}"
}
HTTP 409
[Asserts]
jsonpath "$.error" exists


# Cannot add responses after matching
POST {{BASE_URL}}/api/pings/{{ping2_id}}/responses
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
[Asserts]
jsonpath "$.error" exists


# Confirm hangout - now returns Ping, not Hangout
POST {{BASE_URL}}/api/pings/{{ping2_id}}/confirm
Content-Type: application/json
{
  "user_id": "{{initiator_id}}",
  "timeline": {
    "start": "2024-12-15T18:00:00Z",
    "end": "2024-12-15T21:00:00Z"
  }
}
HTTP 201


# Cannot confirm again
POST {{BASE_URL}}/api/pings/{{ping2_id}}/confirm
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


# Cannot complete ping before activation
POST {{BASE_URL}}/api/pings/{{ping2_id}}/complete
HTTP 409
[Asserts]
jsonpath "$.error" exists


# Activate ping
POST {{BASE_URL}}/api/pings/{{ping2_id}}/activate
HTTP 200


# Cannot activate again
POST {{BASE_URL}}/api/pings/{{ping2_id}}/activate
HTTP 409
[Asserts]
jsonpath "$.error" exists


# Complete ping
POST {{BASE_URL}}/api/pings/{{ping2_id}}/complete
HTTP 200


# Cannot complete again
POST {{BASE_URL}}/api/pings/{{ping2_id}}/complete
HTTP 409
[Asserts]
jsonpath "$.error" exists


# Cannot cancel completed ping
POST {{BASE_URL}}/api/pings/{{ping2_id}}/cancel
Content-Type: application/json
{
  "user_id": "{{initiator_id}}"
}
HTTP 409
[Asserts]
jsonpath "$.error" exists


# ============================================================================
# Already Cancelled
# ============================================================================

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
ping3_id: jsonpath "$.id"


POST {{BASE_URL}}/api/pings/{{ping3_id}}/cancel
Content-Type: application/json
{
  "user_id": "{{initiator_id}}"
}
HTTP 200


# Cannot cancel again
POST {{BASE_URL}}/api/pings/{{ping3_id}}/cancel
Content-Type: application/json
{
  "user_id": "{{initiator_id}}"
}
HTTP 409
[Asserts]
jsonpath "$.error" exists


# Cannot add responses to cancelled ping
POST {{BASE_URL}}/api/pings/{{ping3_id}}/responses
Content-Type: application/json
{
  "user": "{{member1_id}}",
  "answer": true,
  "availability": {
    "earliest": "2024-12-15T14:00:00Z",
    "latest": "2024-12-15T17:00:00Z"
  }
}
HTTP 409
[Asserts]
jsonpath "$.error" exists

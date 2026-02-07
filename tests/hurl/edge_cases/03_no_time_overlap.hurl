# Edge Case: No Time Overlap
# Tests handling when responses have non-overlapping availability windows

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
  "name": "Early Bird"
}
HTTP 201
[Captures]
early_id: jsonpath "$.id"


POST {{BASE_URL}}/api/users
Content-Type: application/json
{
  "name": "Night Owl"
}
HTTP 201
[Captures]
late_id: jsonpath "$.id"


POST {{BASE_URL}}/api/groups
Content-Type: application/json
{
  "name": "Misaligned Schedules",
  "creator_id": "{{initiator_id}}"
}
HTTP 201
[Captures]
group_id: jsonpath "$.id"
invite_code: jsonpath "$.invite_code"


POST {{BASE_URL}}/api/groups/join
Content-Type: application/json
{
  "user_id": "{{early_id}}",
  "invite_code": "{{invite_code}}"
}
HTTP 200


POST {{BASE_URL}}/api/groups/join
Content-Type: application/json
{
  "user_id": "{{late_id}}",
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


# Early bird: available 4pm-6pm
POST {{BASE_URL}}/api/pings/{{ping_id}}/responses
Content-Type: application/json
{
  "user": "{{early_id}}",
  "answer": true,
  "availability": {
    "earliest": "2024-12-15T16:00:00Z",
    "latest": "2024-12-15T18:00:00Z"
  }
}
HTTP 201


# Night owl: available 7pm-11pm (no overlap with early bird)
POST {{BASE_URL}}/api/pings/{{ping_id}}/responses
Content-Type: application/json
{
  "user": "{{late_id}}",
  "answer": true,
  "availability": {
    "earliest": "2024-12-15T19:00:00Z",
    "latest": "2024-12-15T23:00:00Z"
  }
}
HTTP 201


# Trigger matching
POST {{BASE_URL}}/api/pings/{{ping_id}}/match
Content-Type: application/json
{
  "user_id": "{{initiator_id}}"
}
HTTP 200
[Asserts]
jsonpath "$.state" == "no_match"


# Match results show no overlap
GET {{BASE_URL}}/api/pings/{{ping_id}}/match-results
HTTP 200
[Asserts]
jsonpath "$.has_match" == false
jsonpath "$.overlap" == null


# Verify no_match state
GET {{BASE_URL}}/api/pings/{{ping_id}}
HTTP 200
[Asserts]
jsonpath "$.state" == "no_match"

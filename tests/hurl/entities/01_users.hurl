# User CRUD Tests

# Create a new user
POST {{BASE_URL}}/api/users
Content-Type: application/json
{
  "name": "Alice",
  "avatar": "https://example.com/alice.jpg",
  "preferences": {
    "default_distance": 5.0,
    "favorite_areas": ["downtown", "midtown"],
    "home_location": {
      "lat": 40.7128,
      "lng": -74.0060
    }
  }
}
HTTP 201
[Captures]
user_id: jsonpath "$.id"
[Asserts]
jsonpath "$.name" == "Alice"
jsonpath "$.avatar" == "https://example.com/alice.jpg"
jsonpath "$.preferences.default_distance" == 5.0
jsonpath "$.preferences.favorite_areas[0]" == "downtown"
jsonpath "$.preferences.home_location.lat" == 40.7128


# Get user by ID
GET {{BASE_URL}}/api/users/{{user_id}}
HTTP 200
[Asserts]
jsonpath "$.id" == {{user_id}}
jsonpath "$.name" == "Alice"


# Update user
PATCH {{BASE_URL}}/api/users/{{user_id}}
Content-Type: application/json
{
  "name": "Alice Updated",
  "preferences": {
    "default_distance": 10.0
  }
}
HTTP 200
[Asserts]
jsonpath "$.name" == "Alice Updated"
jsonpath "$.preferences.default_distance" == 10.0


# Create user with minimal data
POST {{BASE_URL}}/api/users
Content-Type: application/json
{
  "name": "Bob"
}
HTTP 201
[Captures]
bob_id: jsonpath "$.id"
[Asserts]
jsonpath "$.name" == "Bob"
jsonpath "$.avatar" == null


# Get user's groups (empty initially)
GET {{BASE_URL}}/api/users/{{user_id}}/groups
HTTP 200
[Asserts]
jsonpath "$" count == 0


# Delete user
DELETE {{BASE_URL}}/api/users/{{bob_id}}
HTTP 204


# Verify user is deleted
GET {{BASE_URL}}/api/users/{{bob_id}}
HTTP 404
[Asserts]
jsonpath "$.error" exists
